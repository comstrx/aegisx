use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use std::thread::JoinHandle;
use std::time::Duration;
use openssl::{base64, hash::MessageDigest, pkey::PKey, sign::Signer};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

use crate::core::error::{AppError, AppFail, AppResult};
use crate::module::config::WebhookConfig;

pub struct Webhooks {
    kinds: Vec<String>,
    sender: Option<mpsc::Sender<Value>>,
    accepted: Arc<AtomicU64>,
    delivered: Arc<AtomicU64>,
    failed: Arc<AtomicU64>,
    overflow: AtomicU64,
}

pub struct WebhookGuard { stop: Option<oneshot::Sender<()>>, thread: Option<JoinHandle<()>> }

impl Webhooks {

    pub fn validate_secrets ( config: &WebhookConfig ) -> AppResult<()> {
        if !config.enabled { return Ok(()); }
        for endpoint in &config.endpoints {
            let secret = std::env::var(&endpoint.secret_env).map_err(|_| AppError::invalid("Webhook secret environment variable is missing"))?;
            if !(32..=4096).contains(&secret.len()) { return Err(AppError::invalid("Webhook secrets need 32–4096 bytes")); }
        }
        Ok(())
    }


    pub fn start ( config: &WebhookConfig ) -> AppResult<(Self, Option<WebhookGuard>)> {

        let accepted = Arc::new(AtomicU64::new(0));
        let delivered = Arc::new(AtomicU64::new(0));
        let failed = Arc::new(AtomicU64::new(0));
        let mut service = Self { kinds: Vec::new(), sender: None, accepted, delivered: delivered.clone(), failed: failed.clone(), overflow: AtomicU64::new(0) };
        if !config.enabled { return Ok((service, None)); }
        service.kinds = ["blocked", "risk_detected", "backend_signal", "cancellation_requested"].into_iter()
            .filter(|kind| config.endpoints.iter().any(|endpoint| endpoint.events.is_empty() || endpoint.events.iter().any(|value| value == kind)))
            .map(String::from).collect();
        let mut endpoints = Vec::new();
        for endpoint in &config.endpoints {
            let secret = std::env::var(&endpoint.secret_env).map_err(|_| AppError::invalid("Webhook secret environment variable is missing"))?;
            if secret.len() < 32 || secret.len() > 4096 { return Err(AppError::invalid("Webhook secrets need 32–4096 bytes")); }
            endpoints.push((endpoint.clone(), PKey::hmac(secret.as_bytes()).or_fail("Cannot initialize webhook signing")?));
        }
        let config = config.clone();
        let client = reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_millis(config.timeout_ms)).build().or_fail("Cannot create webhook client")?;
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().or_fail("Cannot create webhook runtime")?;
        let (sender, mut receiver) = mpsc::channel::<Value>(config.queue_capacity);
        let (stop, stopped) = oneshot::channel();
        service.sender = Some(sender);
        let thread = std::thread::Builder::new().name("aegisx-webhooks".into()).spawn(move || runtime.block_on(async move {
            let work = async {
                while let Some(event) = receiver.recv().await {
                    let payload = serde_json::to_vec(&event).expect("JSON event serialization");
                    let event_id = event["event_id"].as_str().unwrap_or("");
                    let kind = event["type"].as_str().unwrap_or("");
                    let mut success = true;
                    for (endpoint, key) in &endpoints {
                        if !endpoint.events.is_empty() && !endpoint.events.iter().any(|value| value == kind) { continue; }
                        let mut sent = false;
                        for attempt in 0..config.attempts {
                            let timestamp = (crate::core::time::now_ms() / 1000).to_string();
                            let prefix = format!("{event_id}.{timestamp}.");
                            let mut signer = Signer::new(MessageDigest::sha256(), key).expect("HMAC SHA256");
                            signer.update(prefix.as_bytes()).expect("HMAC prefix");
                            signer.update(&payload).expect("HMAC payload");
                            let signature = format!("v1,{}", base64::encode_block(&signer.sign_to_vec().expect("HMAC result")));
                            let response = client.post(&endpoint.url).header("content-type", "application/json")
                                .header("webhook-id", event_id).header("webhook-timestamp", timestamp)
                                .header("webhook-signature", signature).body(payload.clone()).send().await;
                            match response {
                                Ok(response) if response.status().is_success() => { sent = true; break; }
                                Ok(response) if !response.status().is_server_error() && response.status().as_u16() != 429 => break,
                                _ => {}
                            }
                            if attempt + 1 < config.attempts {
                                tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
                            }
                        }
                        if !sent { success = false; tracing::warn!(endpoint = %endpoint.name, event_id, "Webhook delivery exhausted"); }
                    }
                    if success { delivered.fetch_add(1, Ordering::Relaxed); } else { failed.fetch_add(1, Ordering::Relaxed); }
                }
            };
            tokio::select! { _ = stopped => {}, _ = work => {} }
        })).or_fail("Cannot start webhook worker")?;

        Ok((service, Some(WebhookGuard { stop: Some(stop), thread: Some(thread) })))

    }

    pub fn emit ( &self, event: Value ) {

        let Some(sender) = &self.sender else { return; };
        if !event["type"].as_str().is_some_and(|kind| self.kinds.iter().any(|value| value == kind)) { return; }
        if sender.try_send(event).is_ok() { self.accepted.fetch_add(1, Ordering::Relaxed); }
        else {
            let count = self.overflow.fetch_add(1, Ordering::Relaxed) + 1;
            if count.is_power_of_two() { tracing::warn!(dropped = count, "Webhook queue unavailable"); }
        }

    }

    pub fn stats ( &self ) -> Value {
        let accepted = self.accepted.load(Ordering::Relaxed);
        let delivered = self.delivered.load(Ordering::Relaxed);
        let failed = self.failed.load(Ordering::Relaxed);
        json!({ "accepted": accepted, "delivered": delivered, "failed": failed,
            "pending": accepted.saturating_sub(delivered + failed), "overflow": self.overflow.load(Ordering::Relaxed) })
    }

}

impl WebhookGuard {
    pub fn finish ( mut self ) -> AppResult<()> {
        if let Some(stop) = self.stop.take() { let _ = stop.send(()); }
        if let Some(thread) = self.thread.take() { thread.join().map_err(|_| AppError::invalid("Webhook worker panicked"))?; }
        Ok(())
    }
}

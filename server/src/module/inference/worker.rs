use std::{sync::{Arc, atomic::Ordering}, thread, time::Instant};
use tokio::sync::mpsc;
use crate::core::error::{AppError, AppFail, AppResult};
use crate::module::config::ModelConfig;
use super::{arch::{Inference, InferenceGuard, Message, Model, Analysis, Counters, Reservation}, Envelope, Features};

impl Inference {
    pub fn start ( config: &ModelConfig, mut model: Model, caches: Arc<crate::module::cache::Caches>, journal: Option<(&std::path::Path,&str,crate::core::WriteGate)> ) -> AppResult<(Self, InferenceGuard)> {
        let mut journal=journal.map(|(path,synchronous,gate)|super::journal::Journal::open(path,&model.info.artifact_sha256,synchronous,gate)).transpose()?;
        let journal_enabled=journal.is_some();
        let recovered=journal.as_mut().map(|journal|journal.recover(&mut model)).transpose()?.unwrap_or(0);
        let (sender, mut receiver)=mpsc::channel(config.queue_capacity);
        let schema=Features::load()?;
        let max_age=config.max_queue_age_ms;
        let counters=Arc::new(Counters::default());
        counters.recovered.store(recovered,Ordering::Relaxed);
        let stats=counters.clone();
        let thread=thread::Builder::new().name("aegisx-inference".into()).spawn(move || {
            let mut stopping=false;
            while !stopping {
                let Some(message)=receiver.blocking_recv() else {break;};
                let mut jobs=Vec::with_capacity(32);
                match message {
                    Message::Stop=>break,
                    Message::Background(envelope,queued,complete)=>jobs.push(Pending::new(envelope,queued,complete,&schema)),
                }
                // Drain available work only: no batching timer delays an idle request.
                while jobs.len()<32 {
                    match receiver.try_recv() {
                        Ok(Message::Background(envelope,queued,complete))=>jobs.push(Pending::new(envelope,queued,complete,&schema)),
                        Ok(Message::Stop)=>{stopping=true;break;},
                        Err(_)=>break,
                    }
                }
                stats.processing.store(jobs.len() as u64,Ordering::Relaxed);
                let prepared=jobs.iter().enumerate().filter_map(|(index,job)|{
                    (journal_enabled && job.envelope.journal && job.queued.elapsed().as_millis() as u64<=max_age)
                        .then_some(job.features).flatten().map(|features|(index,(job.envelope.request_id.clone(),features)))
                }).collect::<Vec<_>>();
                let input=prepared.iter().map(|(_,value)|value.clone()).collect::<Vec<_>>();
                let journal_start=Instant::now();
                if let Some(journal)=&mut journal && !input.is_empty() {
                    loop {
                        match journal.prepare(&input) {
                            Ok(ids)=>{
                                stats.journal_unhealthy.store(false,Ordering::Release);
                                for ((index,_),id) in prepared.iter().zip(ids) {jobs[*index].journal_id=Some(id);}
                                break;
                            }
                            Err(error)=>{
                                let attempts=stats.journal_failures.fetch_add(1,Ordering::Relaxed)+1;
                                stats.journal_unhealthy.store(true,Ordering::Release);
                                if attempts.is_power_of_two() {tracing::warn!(%error,"Analysis journal unavailable; retaining batch");}
                                if stats.stopping.load(Ordering::Acquire) {break;}
                                thread::sleep(std::time::Duration::from_millis(50));
                            }
                        }
                    }
                }
                let journal_us=journal_start.elapsed().as_micros() as u64;
                stats.journal_us.fetch_add(journal_us,Ordering::Relaxed);
                let per_job_journal_us=journal_us/jobs.len() as u64;
                let mut results=Vec::new();
                for job in jobs {
                    let queue_ms=job.queued.elapsed().as_millis() as u64;
                    let expired=queue_ms>max_age;
                    let start=Instant::now();
                    let durable=job.envelope.journal && journal_enabled;
                    let score=if expired || (durable && job.journal_id.is_none()) {None}
                        else {job.features.and_then(|_|score(&mut model,&caches,job.input.as_ref().expect("prepared input"),job.envelope.cache_scores))};
                    let inference_us=job.normalization_us+start.elapsed().as_micros() as u64;
                    stats.total_us.fetch_add(inference_us,Ordering::Relaxed);
                    stats.last_us.store(inference_us,Ordering::Relaxed);
                    if expired {stats.expired.fetch_add(1,Ordering::Relaxed);}
                    else if score.is_none() {stats.failed.fetch_add(1,Ordering::Relaxed);}
                    (job.complete)(Analysis {inputs:if model.info.input_schema.is_some() {job.input.as_ref().map_or(serde_json::Value::Null,super::Input::summary)} else {serde_json::json!({"schema":"numeric-v4","numeric_features":super::FEATURE_COUNT,"raw_content_persisted":false,"recovery":"observe_only_when_compatible"})},scores:score,features:job.features.unwrap_or([0.0; super::FEATURE_COUNT]),score:score.map(|scores|scores.risk),queue_ms,inference_us,journal_us:per_job_journal_us,expired});
                    if let Some(id)=job.journal_id {results.push((id,if expired {"expired"} else if score.is_some() {"completed"} else {"failed"},score.map(|scores|scores.risk)));}
                    stats.finished.fetch_add(1,Ordering::Relaxed);
                    stats.processing.fetch_sub(1,Ordering::Relaxed);
                }
                if let Some(journal)=&mut journal && let Err(error)=journal.complete(&results) {
                    stats.journal_failures.fetch_add(1,Ordering::Relaxed);
                    tracing::warn!(%error,"Analysis completion pending; recovery will observe only");
                }
            }
        }).or_fail("Cannot start inference worker")?;
        Ok((Self {journal_enabled,sender:sender.clone(),counters:counters.clone()},InferenceGuard {sender,counters,thread:Some(thread)}))
    }
    fn ready ( &self ) -> bool {
        !self.counters.stopping.load(Ordering::Acquire) && !self.counters.journal_unhealthy.load(Ordering::Acquire)
    }
    pub fn try_reserve ( &self ) -> Option<Reservation> {
        if !self.ready() { return None; }
        self.sender.clone().try_reserve_owned().ok().map(|permit|Reservation {permit})
    }
    pub async fn wait_reserve ( &self ) -> Option<Reservation> {
        if !self.ready() {return None;}
        let permit=self.sender.clone().reserve_owned().await.ok()?;
        self.ready().then_some(Reservation {permit})
    }
    pub fn reservation_failed ( &self ) { self.counters.rejected.fetch_add(1,Ordering::Relaxed); }
    pub fn reserve_completion ( &self ) -> Option<Reservation> {
        let permit=self.sender.clone().try_reserve_owned().ok();
        if permit.is_none() {self.counters.dropped.fetch_add(1,Ordering::Relaxed);}
        permit.map(|permit|Reservation {permit})
    }
    pub fn background ( &self, envelope: Envelope, slot: Option<Reservation>, complete: impl FnOnce(Analysis) + Send + 'static ) -> bool {
        let message=Message::Background(Box::new(envelope), Instant::now(), Box::new(complete));
        if let Some(slot)=slot { slot.permit.send(message); }
        else if self.sender.try_send(message).is_err() {
            self.counters.dropped.fetch_add(1, Ordering::Relaxed); return false;
        }
        self.counters.submitted.fetch_add(1, Ordering::Relaxed); true
    }
    pub fn stats ( &self ) -> serde_json::Value {
        let read = |counter: &std::sync::atomic::AtomicU64| counter.load(Ordering::Relaxed);
        serde_json::json!({"journal_enabled":self.journal_enabled,"journal_healthy":!self.counters.journal_unhealthy.load(Ordering::Acquire),"journal_us":read(&self.counters.journal_us),"recovered":read(&self.counters.recovered),"journal_failures":read(&self.counters.journal_failures),"submitted":read(&self.counters.submitted),"finished":read(&self.counters.finished),
            "dropped":read(&self.counters.dropped),"pressure_rejections":read(&self.counters.rejected),"expired":read(&self.counters.expired),"failed":read(&self.counters.failed),
            "total_us":read(&self.counters.total_us),"last_us":read(&self.counters.last_us),
            "queued":self.sender.max_capacity()-self.sender.capacity(),"capacity":self.sender.max_capacity(),"processing":read(&self.counters.processing)})
    }
}
struct Pending {
    envelope: Box<Envelope>, queued: Instant, complete: Box<dyn FnOnce(Analysis)+Send>,
    features: Option<super::Vector>, input: Option<super::Input>, normalization_us: u64, journal_id: Option<i64>,
}
impl Pending {
    fn new ( mut envelope: Box<Envelope>, queued: Instant, complete: Box<dyn FnOnce(Analysis)+Send>, schema: &Features ) -> Self {
        let start=Instant::now();
        let features=schema.normalize(envelope.raw());
        let input=features.map(|features|super::Input::from_envelope(features,&envelope));
        envelope.sample=Vec::new();envelope.response_sample=Vec::new();envelope.backend_events=Vec::new();
        let normalization_us=start.elapsed().as_micros() as u64;
        Self {envelope,queued,complete,features,input,normalization_us,journal_id:None}
    }
}
fn score ( model: &mut Model, caches: &crate::module::cache::Caches, input: &super::Input, cached: bool ) -> Option<crate::core::domain::RiskScores> {
    if !cached {return model.predict(input).ok();}
    let key=input.key(&model.info.artifact_sha256);
    if let Some(score)=caches.scores.get(&key) {caches.score_hits.fetch_add(1,Ordering::Relaxed);return Some(score);}
    let score=model.predict(input).ok()?;
    caches.scores.insert(key,score);
    Some(score)
}
impl InferenceGuard {
    pub fn finish ( mut self ) -> AppResult<()> {
        self.counters.stopping.store(true,Ordering::Release);
        let _ = self.sender.blocking_send(Message::Stop);
        if let Some(thread) = self.thread.take() { thread.join().map_err(|_| AppError::invalid("Inference worker panicked"))?; }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc as sync, time::Duration};
    use crate::module::{cache::Caches,config::CacheConfig};
    fn envelope () -> Envelope {
        Envelope {journal:false,cache_scores:false,admission:[0.0;16],sample:Vec::new(),sample_seen:0,response_sample:Vec::new(),response_seen:0,response_available:false,events_truncated:false,outcome:[0.0;8],request_id:String::new(),backend_events:Vec::new()}
    }
    #[test]
    fn queue_is_bounded_and_old_jobs_cannot_produce_verdicts () {
        let config=ModelConfig {queue_capacity:1,max_queue_age_ms:1,..Default::default()};
        let (worker,guard)=Inference::start(&config,Model::load().unwrap(),Arc::new(Caches::new(&CacheConfig::default())),None).unwrap();
        let (entered,ready)=sync::channel();
        let (release,held)=sync::channel();
        let (finished,result)=sync::channel();
        assert!(worker.background(envelope(),None,move |_| {
            entered.send(()).unwrap();
            held.recv_timeout(Duration::from_secs(3)).unwrap();
        }));
        ready.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(worker.background(envelope(),None,move |analysis| {finished.send((analysis.expired,analysis.score)).unwrap();}));
        assert!(!worker.background(envelope(),None,move |_| panic!("Overflowed job must not execute")));
        std::thread::sleep(Duration::from_millis(20));
        release.send(()).unwrap();
        assert_eq!(result.recv_timeout(Duration::from_secs(3)).unwrap(),(true,None));
        guard.finish().unwrap();
        assert_eq!(worker.stats()["submitted"],2);
        assert_eq!(worker.stats()["dropped"],1);
    }
}

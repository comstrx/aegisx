use std::future::Future;
use serde_json::json;
use super::{Context, Proxy};

impl Proxy {
    pub(super) async fn wait_for<T> ( &self, context: &mut Context, resource: &str, future: impl Future<Output=Option<T>> ) -> Option<T> {
        if !self.queue.enabled() { return None; }
        context.waited = true;
        self.record(context, "queued", |_| json!({"resource":resource,"forwarding_started":false}));
        let result = self.queue.wait(&mut context.wait_deadline, future).await;
        self.record(context, "queue_released", |_| json!({"resource":resource,"acquired":result.is_some()}));
        result
    }
    pub(super) async fn queued_restriction ( &self, context: &mut Context ) -> Option<(u16,String)> {
        if !context.waited { return None; }
        let (Some(actor),Some(route)) = (context.actor,&context.route) else {return None;};
        let result=self.engine.restriction(actor,&context.snapshot,route).await;
        if let Some(status)=result.status {
            context.model_rejected=result.model_rejected;
            context.inspection.denial_cached=result.denial_cached;
            return Some((status,result.reason));
        }
        None
    }
}

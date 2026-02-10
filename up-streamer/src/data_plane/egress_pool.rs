//! Egress-route worker pool and refcounted transport ownership.

use crate::control_plane::transport_identity::TransportIdentityKey;
use crate::data_plane::egress_worker::EgressRouteWorker;
use crate::observability::{events, fields};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast::Sender;
use tokio::sync::Mutex;
use tracing::{debug, warn};
use up_rust::{UMessage, UTransport};

const COMPONENT: &str = "egress_pool";

/// Per-transport egress worker binding state.
pub(crate) struct EgressRouteBinding {
    pub(crate) ref_count: usize,
    pub(crate) worker: EgressRouteWorker,
    pub(crate) sender: Sender<Arc<UMessage>>,
}

/// Refcounted registry of egress workers keyed by transport identity.
pub(crate) struct EgressRoutePool {
    message_queue_size: usize,
    pub(crate) workers: Mutex<HashMap<TransportIdentityKey, EgressRouteBinding>>,
}

impl EgressRoutePool {
    /// Creates an empty egress route pool.
    pub(crate) fn new(message_queue_size: usize) -> Self {
        Self {
            message_queue_size,
            workers: Mutex::new(HashMap::new()),
        }
    }

    /// Attaches one route to an egress transport, reusing worker state when possible.
    pub(crate) async fn attach_route(
        &mut self,
        out_transport: Arc<dyn UTransport>,
        route_label: &str,
    ) -> Sender<Arc<UMessage>> {
        let out_transport_key = TransportIdentityKey::new(out_transport.clone());

        let mut egress_workers = self.workers.lock().await;

        if let Some(slot) = egress_workers.get_mut(&out_transport_key) {
            slot.ref_count += 1;
            debug!(
                event = events::EGRESS_WORKER_REUSE,
                component = COMPONENT,
                worker_id = slot.worker.worker_id(),
                route_label,
                ref_count = slot.ref_count,
                "reused pooled egress worker"
            );
            return slot.sender.clone();
        }

        let (tx, rx) = tokio::sync::broadcast::channel(self.message_queue_size);
        let worker = EgressRouteWorker::new(out_transport, rx);
        let worker_id = worker.worker_id().to_string();
        let sender = tx.clone();

        egress_workers.insert(
            out_transport_key,
            EgressRouteBinding {
                ref_count: 1,
                worker,
                sender: tx,
            },
        );

        debug!(
            event = events::EGRESS_WORKER_CREATE,
            component = COMPONENT,
            worker_id,
            route_label,
            ref_count = 1,
            "created pooled egress worker"
        );

        sender
    }

    /// Detaches one route from an egress transport and drops worker state at refcount zero.
    pub(crate) async fn detach_route(&mut self, out_transport: Arc<dyn UTransport>) {
        let out_transport_key = TransportIdentityKey::new(out_transport.clone());

        let mut egress_workers = self.workers.lock().await;

        let active_num = {
            let Some(slot) = egress_workers.get_mut(&out_transport_key) else {
                warn!(
                    event = events::EGRESS_WORKER_REMOVE,
                    component = COMPONENT,
                    worker_id = fields::NONE,
                    ref_count = 0,
                    reason = "missing_transport_binding",
                    "unable to detach missing egress worker"
                );
                return;
            };

            slot.ref_count -= 1;
            slot.ref_count
        };

        if active_num == 0 {
            let removed = egress_workers.remove(&out_transport_key);
            if let Some(binding) = removed {
                debug!(
                    event = events::EGRESS_WORKER_REMOVE,
                    component = COMPONENT,
                    worker_id = binding.worker.worker_id(),
                    ref_count = 0,
                    worker_thread = binding.worker.runtime_thread(),
                    "removed pooled egress worker"
                );
            } else {
                warn!(
                    event = events::EGRESS_WORKER_REMOVE,
                    component = COMPONENT,
                    worker_id = fields::NONE,
                    ref_count = 0,
                    reason = "missing_transport_binding",
                    "egress worker remove observed missing binding"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EgressRoutePool;
    use async_trait::async_trait;
    use std::sync::Arc;
    use up_rust::{UCode, UListener, UMessage, UStatus, UTransport, UUri};

    struct NoopTransport;

    #[async_trait]
    impl UTransport for NoopTransport {
        async fn send(&self, _message: UMessage) -> Result<(), UStatus> {
            Ok(())
        }

        async fn receive(
            &self,
            _source_filter: &UUri,
            _sink_filter: Option<&UUri>,
        ) -> Result<UMessage, UStatus> {
            Err(UStatus::fail_with_code(
                UCode::UNIMPLEMENTED,
                "not used in tests",
            ))
        }

        async fn register_listener(
            &self,
            _source_filter: &UUri,
            _sink_filter: Option<&UUri>,
            _listener: Arc<dyn UListener>,
        ) -> Result<(), UStatus> {
            Ok(())
        }

        async fn unregister_listener(
            &self,
            _source_filter: &UUri,
            _sink_filter: Option<&UUri>,
            _listener: Arc<dyn UListener>,
        ) -> Result<(), UStatus> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn attach_route_reuses_queue_and_increments_refcount() {
        let mut pool = EgressRoutePool::new(8);
        let transport: Arc<dyn UTransport> = Arc::new(NoopTransport);

        let sender_a = pool.attach_route(transport.clone(), "route-a").await;
        let sender_b = pool.attach_route(transport, "route-b").await;

        let workers = pool.workers.lock().await;
        assert_eq!(workers.len(), 1);
        let slot = workers
            .values()
            .next()
            .expect("single egress worker binding");
        assert_eq!(slot.ref_count, 2);
        assert!(!slot.worker.worker_id().is_empty());
        assert!(sender_a.same_channel(&sender_b));
    }

    #[tokio::test]
    async fn detach_route_drops_worker_when_refcount_reaches_zero() {
        let mut pool = EgressRoutePool::new(8);
        let transport: Arc<dyn UTransport> = Arc::new(NoopTransport);

        pool.attach_route(transport.clone(), "route-a").await;
        pool.attach_route(transport.clone(), "route-b").await;

        pool.detach_route(transport.clone()).await;
        assert_eq!(pool.workers.lock().await.len(), 1);

        pool.detach_route(transport).await;
        assert!(pool.workers.lock().await.is_empty());
    }
}

//! Egress worker abstraction that forwards queued messages on output transports.

use crate::runtime::worker_runtime::spawn_route_dispatch_loop;
use std::ops::Deref;
use std::sync::Arc;
use tokio::sync::broadcast::Receiver;
use tracing::{debug, info, trace, warn};
use up_rust::{UMessage, UTransport, UUID};

const EGRESS_ROUTE_WORKER_TAG: &str = "EgressRouteWorker:";
const EGRESS_ROUTE_WORKER_FN_RUN_LOOP_TAG: &str = "run_loop():";

/// Worker state that owns the spawned route-dispatch thread handle.
pub(crate) struct EgressRouteWorker {
    join_handle: std::thread::JoinHandle<()>,
}

impl EgressRouteWorker {
    /// Spawns a dedicated runtime thread for one egress transport dispatch loop.
    pub(crate) fn new(
        out_transport: Arc<dyn UTransport>,
        message_receiver: Receiver<Arc<UMessage>>,
    ) -> Self {
        let out_transport_clone = out_transport.clone();
        let message_receiver_clone = message_receiver.resubscribe();

        let join_handle = spawn_route_dispatch_loop(
            out_transport_clone,
            message_receiver_clone,
            move |out_transport, message_receiver| async move {
                trace!("Within blocked runtime");
                Self::route_dispatch_loop(
                    UUID::build().to_hyphenated_string(),
                    out_transport,
                    message_receiver,
                )
                .await;
                info!("Broke out of loop! You probably dropped the UPClientVsomeip");
            },
        );

        Self { join_handle }
    }

    /// Returns the backing runtime thread ID for diagnostics.
    pub(crate) fn thread_id(&self) -> std::thread::ThreadId {
        self.join_handle.thread().id()
    }

    /// Executes the dispatch loop by forwarding each received message to egress transport.
    pub(crate) async fn route_dispatch_loop(
        id: String,
        out_transport: Arc<dyn UTransport>,
        mut message_receiver: Receiver<Arc<UMessage>>,
    ) {
        while let Ok(msg) = message_receiver.recv().await {
            debug!(
                "{}:{}:{} Attempting send of message: {:?}",
                id, EGRESS_ROUTE_WORKER_TAG, EGRESS_ROUTE_WORKER_FN_RUN_LOOP_TAG, msg
            );
            let send_res = out_transport.send(msg.deref().clone()).await;
            if let Err(err) = send_res {
                warn!(
                    "{}:{}:{} Sending on out_transport failed: {:?}",
                    id, EGRESS_ROUTE_WORKER_TAG, EGRESS_ROUTE_WORKER_FN_RUN_LOOP_TAG, err
                );
            } else {
                debug!(
                    "{}:{}:{} Sending on out_transport succeeded",
                    id, EGRESS_ROUTE_WORKER_TAG, EGRESS_ROUTE_WORKER_FN_RUN_LOOP_TAG
                );
            }
        }
    }
}

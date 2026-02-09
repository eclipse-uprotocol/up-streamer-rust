//! Runtime helper for spawning egress route dispatch loops.

use std::sync::Arc;
use std::thread;
use tokio::runtime::Builder;
use tokio::sync::broadcast::Receiver;
use up_rust::{UMessage, UTransport};

const EGRESS_ROUTE_RUNTIME_THREAD_NAME: &str = "up-streamer-egress-route-runtime";

pub(crate) fn spawn_route_dispatch_loop<F, Fut>(
    out_transport: Arc<dyn UTransport>,
    message_receiver: Receiver<Arc<UMessage>>,
    run_loop: F,
) -> thread::JoinHandle<()>
where
    F: FnOnce(Arc<dyn UTransport>, Receiver<Arc<UMessage>>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    thread::Builder::new()
        .name(EGRESS_ROUTE_RUNTIME_THREAD_NAME.to_string())
        .spawn(move || {
            let runtime = Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create egress route Tokio runtime");

            runtime.block_on(run_loop(out_transport, message_receiver));
        })
        .expect("Failed to spawn egress route runtime thread")
}

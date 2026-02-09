//! Ingress-route listener adapter that receives messages and feeds egress dispatch.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::broadcast::Sender;
use tracing::{debug, error};
use up_rust::{UListener, UMessage, UPayloadFormat};

const INGRESS_ROUTE_LISTENER_TAG: &str = "IngressRouteListener:";
const INGRESS_ROUTE_LISTENER_FN_ON_RECEIVE_TAG: &str = "on_receive():";

#[derive(Clone)]
pub(crate) struct IngressRouteListener {
    route_id: String,
    sender: Sender<Arc<UMessage>>,
}

impl IngressRouteListener {
    pub(crate) fn new(route_id: &str, sender: Sender<Arc<UMessage>>) -> Self {
        Self {
            route_id: route_id.to_string(),
            sender,
        }
    }
}

#[async_trait]
impl UListener for IngressRouteListener {
    async fn on_receive(&self, msg: UMessage) {
        debug!(
            "{}:{}:{} Received message: {:?}",
            self.route_id,
            INGRESS_ROUTE_LISTENER_TAG,
            INGRESS_ROUTE_LISTENER_FN_ON_RECEIVE_TAG,
            &msg
        );

        if msg.attributes.payload_format.enum_value_or_default()
            == UPayloadFormat::UPAYLOAD_FORMAT_SHM
        {
            debug!(
                "{}:{}:{} Received message with type UPAYLOAD_FORMAT_SHM, which is not supported. A pointer to shared memory will not be usable on another device. UAttributes: {:#?}",
                self.route_id,
                INGRESS_ROUTE_LISTENER_TAG,
                INGRESS_ROUTE_LISTENER_FN_ON_RECEIVE_TAG,
                &msg.attributes
            );
            return;
        }

        if let Err(e) = self.sender.send(Arc::new(msg)) {
            error!(
                "{}:{}:{} Unable to send message to worker pool: {e:?}",
                self.route_id, INGRESS_ROUTE_LISTENER_TAG, INGRESS_ROUTE_LISTENER_FN_ON_RECEIVE_TAG,
            );
        }
    }
}

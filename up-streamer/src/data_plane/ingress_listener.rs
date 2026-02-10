//! Ingress-route listener adapter that receives messages and feeds egress dispatch.

use crate::observability::{events, fields};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::broadcast::Sender;
use tracing::{debug, error, Level};
use up_rust::{UListener, UMessage, UPayloadFormat};

const COMPONENT: &str = "ingress_listener";

struct FormattedMessageFields {
    msg_id: String,
    msg_type: String,
    src: String,
    sink: String,
}

impl FormattedMessageFields {
    fn from_message(message: &UMessage) -> Self {
        Self {
            msg_id: fields::format_message_id(message),
            msg_type: fields::format_message_type(message),
            src: fields::format_source_uri(message),
            sink: fields::format_sink_uri(message),
        }
    }
}

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
        let route_label = self.route_id.as_str();
        let formatted_fields =
            tracing::enabled!(Level::DEBUG).then(|| FormattedMessageFields::from_message(&msg));

        if let Some(fields) = formatted_fields.as_ref() {
            debug!(
                event = events::INGRESS_RECEIVE,
                component = COMPONENT,
                route_label,
                msg_id = fields.msg_id.as_str(),
                msg_type = fields.msg_type.as_str(),
                src = fields.src.as_str(),
                sink = fields.sink.as_str(),
                "received ingress message"
            );
        }

        if msg.attributes.payload_format.enum_value_or_default()
            == UPayloadFormat::UPAYLOAD_FORMAT_SHM
        {
            if let Some(fields) = formatted_fields.as_ref() {
                debug!(
                    event = events::INGRESS_DROP_UNSUPPORTED_PAYLOAD,
                    component = COMPONENT,
                    route_label,
                    msg_id = fields.msg_id.as_str(),
                    msg_type = fields.msg_type.as_str(),
                    src = fields.src.as_str(),
                    sink = fields.sink.as_str(),
                    reason = "unsupported_payload_format_shm",
                    "dropping unsupported shared-memory payload"
                );
            }
            return;
        }

        if let Err(e) = self.sender.send(Arc::new(msg)) {
            error!(
                event = events::INGRESS_SEND_TO_POOL_FAILED,
                component = COMPONENT,
                route_label,
                err = ?e,
                "unable to send message to egress pool"
            );
        }
    }
}

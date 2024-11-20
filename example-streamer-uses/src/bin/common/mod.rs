use async_trait::async_trait;
use hello_world_protos::{
    hello_world_service::{HelloRequest, HelloResponse},
    hello_world_topics::Timer,
};
use log::{debug, error, info};
use protobuf::Message;
use std::sync::Arc;
use up_rust::{UListener, UMessage, UMessageBuilder, UTransport};

#[allow(dead_code)]
pub(crate) struct ServiceResponseListener;

#[async_trait]
impl UListener for ServiceResponseListener {
    async fn on_receive(&self, msg: UMessage) {
        info!("ServiceResponseListener: Received a message: {msg:?}");

        let Some(payload_bytes) = msg.payload else {
            panic!("No payload bytes");
        };

        let Ok(hello_response) = HelloResponse::parse_from_bytes(&payload_bytes) else {
            panic!("Unable to parse into HelloResponse");
        };

        debug!("Here we received response: {hello_response:?}");
    }
}

#[allow(dead_code)]
pub(crate) struct ServiceRequestResponder {
    client: Arc<dyn UTransport>,
}
impl ServiceRequestResponder {
    #[allow(dead_code)]
    pub(crate) fn new(client: Arc<dyn UTransport>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl UListener for ServiceRequestResponder {
    async fn on_receive(&self, msg: UMessage) {
        info!("ServiceResponseListener: Received a message: {msg:?}");

        let Some(payload_bytes) = msg.payload else {
            panic!("No bytes available");
        };
        let hello_request = match HelloRequest::parse_from_bytes(&payload_bytes) {
            Ok(hello_request) => {
                debug!("hello_request: {hello_request:?}");
                hello_request
            }
            Err(err) => {
                error!("Unable to parse HelloRequest: {err:?}");
                return;
            }
        };

        let hello_response = HelloResponse {
            message: format!("The response to the request: {}", hello_request.name),
            ..Default::default()
        };

        let response_msg = UMessageBuilder::response_for_request(msg.attributes.as_ref().unwrap())
            .build_with_protobuf_payload(&hello_response)
            .unwrap();
        info!("Sending Response message:\n{:?}", &response_msg);
        self.client.send(response_msg).await.unwrap();
    }
}

#[allow(dead_code)]
pub(crate) struct PublishReceiver;

#[async_trait]
impl UListener for PublishReceiver {
    async fn on_receive(&self, msg: UMessage) {
        info!("PublishReceiver: Received a message: {msg:?}");

        let Some(payload_bytes) = msg.payload else {
            panic!("No bytes available");
        };
        match Timer::parse_from_bytes(&payload_bytes) {
            Ok(timer_message) => {
                debug!("timer: {timer_message:?}");
            }
            Err(err) => {
                error!("Unable to parse Timer Message: {err:?}");
            }
        };
    }
}

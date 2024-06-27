use async_trait::async_trait;
use hello_world_protos::hello_world_service::{HelloRequest, HelloResponse};
use log::error;
use protobuf::Message;
use std::str::FromStr;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use up_rust::{UListener, UMessage, UMessageBuilder, UPayloadFormat, UStatus, UTransport, UUri};
use up_transport_zenoh::UPClientZenoh;
use zenoh::config::{Config, EndPoint};

const SERVICE_AUTHORITY: &str = "linux";
const SERVICE_UE_ID: u16 = 0x1236;
const SERVICE_UE_VERSION_MAJOR: u8 = 1;
const SERVICE_RESOURCE_ID: u16 = 0x0896;

const CLIENT_AUTHORITY: &str = "me_authority";
const CLIENT_UE_ID: u16 = 0x5678;
const CLIENT_UE_VERSION_MAJOR: u8 = 1;
const CLIENT_RESOURCE_ID: u16 = 0;

const REQUEST_TTL: u32 = 1000;

struct ServiceRequestResponder {
    client: Arc<dyn UTransport>,
}
impl ServiceRequestResponder {
    pub fn new(client: Arc<dyn UTransport>) -> Self {
        Self { client }
    }
}
#[async_trait]
impl UListener for ServiceRequestResponder {
    async fn on_receive(&self, msg: UMessage) {
        println!("ServiceRequestResponder: Received a message: {msg:?}");

        let Some(payload_bytes) = msg.payload else {
            panic!("No bytes available");
        };
        let hello_request = match HelloRequest::parse_from_bytes(&payload_bytes) {
            Ok(hello_request) => {
                println!("hello_request: {hello_request:?}");
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
        self.client.send(response_msg).await.unwrap();
    }

    async fn on_error(&self, err: UStatus) {
        println!("ServiceRequestResponder: Encountered an error: {err:?}");
    }
}

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    println!("uE_service");

    // TODO: Probably make somewhat configurable?
    let mut zenoh_config = Config::default();
    // Specify the address to listen on using IPv4
    let ipv4_endpoint = EndPoint::from_str("tcp/0.0.0.0:7445");

    // Add the IPv4 endpoint to the Zenoh configuration
    zenoh_config
        .listen
        .endpoints
        .push(ipv4_endpoint.expect("FAIL"));
    // TODO: Add error handling if we fail to create a UPClientZenoh
    let service: Arc<dyn UTransport> = Arc::new(
        UPClientZenoh::new(zenoh_config, "linux".to_string())
            .await
            .unwrap(),
    );

    let source_filter = UUri {
        authority_name: "*".to_string(),
        ue_id: 0x0000_FFFF,
        ue_version_major: 0xFF,
        resource_id: 0xFFFF,
        ..Default::default()
    };
    let sink_filter = UUri {
        authority_name: SERVICE_AUTHORITY.to_string(),
        ue_id: SERVICE_UE_ID as u32,
        ue_version_major: SERVICE_UE_VERSION_MAJOR as u32,
        resource_id: SERVICE_RESOURCE_ID as u32,
        ..Default::default()
    };

    let service_request_responder: Arc<dyn UListener> =
        Arc::new(ServiceRequestResponder::new(service.clone()));
    service
        .register_listener(
            &source_filter,
            Some(&sink_filter),
            service_request_responder.clone(),
        )
        .await?;

    thread::park();
    Ok(())
}

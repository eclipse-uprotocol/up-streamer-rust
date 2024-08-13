/********************************************************************************
 * Copyright (c) 2024 Contributors to the Eclipse Foundation
 *
 * See the NOTICE file(s) distributed with this work for additional
 * information regarding copyright ownership.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Apache License Version 2.0 which is available at
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

use async_trait::async_trait;
use clap::Parser;
use hello_world_protos::hello_world_service::{HelloRequest, HelloResponse};
use log::error;
use protobuf::Message;
use std::str::FromStr;
use std::sync::Arc;
use std::thread;
use up_rust::{UListener, UMessage, UMessageBuilder, UStatus, UTransport, UUri};
use up_transport_zenoh::UPClientZenoh;
use zenoh::config::{Config, EndPoint};

const SERVICE_AUTHORITY: &str = "linux";
const SERVICE_UE_ID: u16 = 0x1236;
const SERVICE_UE_VERSION_MAJOR: u8 = 1;
const SERVICE_RESOURCE_ID: u16 = 0x0896;

struct ServiceRequestResponder {
    client: Arc<dyn UTransport>,
}
impl ServiceRequestResponder {
    pub fn new(client: Arc<dyn UTransport>) -> Self {
        Self { client }
    }
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The endpoint for Zenoh client to connect to
    #[arg(short, long, default_value = "tcp/0.0.0.0:7443")]
    endpoint: String,
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

    let args = Args::parse();

    println!("uE_service");

    // TODO: Probably make somewhat configurable?
    let mut zenoh_config = Config::default();

    if !args.endpoint.is_empty() {
        // Specify the address to listen on using IPv4
        let ipv4_endpoint = EndPoint::from_str(args.endpoint.as_str());

        // Add the IPv4 endpoint to the Zenoh configuration
        zenoh_config
            .listen
            .endpoints
            .push(ipv4_endpoint.expect("FAIL"));
    }

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

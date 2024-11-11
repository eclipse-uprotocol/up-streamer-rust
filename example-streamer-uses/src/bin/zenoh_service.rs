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

mod common;

use async_trait::async_trait;
use hello_world_protos::hello_world_service::{HelloRequest, HelloResponse};
use log::error;
use protobuf::Message;
use std::sync::Arc;
use std::thread;
use up_rust::{UListener, UMessage, UMessageBuilder, UStatus, UTransport, UUri};
use up_transport_zenoh::UPTransportZenoh;

const SERVICE_AUTHORITY: &str = "ecu_authority";
const SERVICE_UE_ID: u32 = 0x1236;
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
}

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    println!("zenoh_service");

    // We set the source filter to "any" so that we process messages from all device that send some.
    let source_filter = UUri::any();
    // The sink filter gets specified so that we only process messages directed at this entity.
    let sink_filter = UUri::try_from_parts(
        SERVICE_AUTHORITY,
        SERVICE_UE_ID,
        SERVICE_UE_VERSION_MAJOR,
        SERVICE_RESOURCE_ID,
    )
    .unwrap();

    // The URI of the service entity
    let service_uri = UUri::try_from_parts(
        SERVICE_AUTHORITY,
        SERVICE_UE_ID,
        SERVICE_UE_VERSION_MAJOR,
        0,
    )
    .unwrap();

    let zenoh_config = common::get_zenoh_config();

    let service: Arc<dyn UTransport> = Arc::new(
        UPTransportZenoh::new(zenoh_config, service_uri)
            .await
            .unwrap(),
    );

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

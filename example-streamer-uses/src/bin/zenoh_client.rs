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
use hello_world_protos::hello_world_service::{HelloRequest, HelloResponse};
use log::{debug, info};
use protobuf::Message;
use std::sync::Arc;
use std::time::Duration;
use up_rust::{UListener, UMessage, UMessageBuilder, UStatus, UTransport, UUri};
use up_transport_zenoh::UPTransportZenoh;

mod common;

const SERVICE_AUTHORITY: &str = "cloud_authority";
const SERVICE_UE_ID: u32 = 0x1236;
const SERVICE_UE_VERSION_MAJOR: u8 = 1;
const SERVICE_RESOURCE_ID: u16 = 0x0896;

const CLIENT_AUTHORITY: &str = "ecu_authority";
const CLIENT_UE_ID: u32 = 0x5678;
const CLIENT_UE_VERSION_MAJOR: u8 = 1;
const CLIENT_RESOURCE_ID: u16 = 0;

const REQUEST_TTL: u32 = 1000;

struct ServiceResponseListener;

#[async_trait]
impl UListener for ServiceResponseListener {
    async fn on_receive(&self, msg: UMessage) {
        debug!("ServiceResponseListener: Received a message: {msg:?}");

        let Some(payload_bytes) = msg.payload else {
            panic!("No payload bytes");
        };

        let Ok(hello_response) = HelloResponse::parse_from_bytes(&payload_bytes) else {
            panic!("Unable to parse into HelloResponse");
        };

        info!("Here we received response: {hello_response:?}");
    }
}

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    info!("Stared zenoh_client.");

    // Source represents the client (specifically the topic that the client sends to)
    let source = UUri::try_from_parts(
        CLIENT_AUTHORITY,
        CLIENT_UE_ID,
        CLIENT_UE_VERSION_MAJOR,
        CLIENT_RESOURCE_ID,
    )
    .unwrap();
    // Sink is the entity which the streamer should rout our messages to.
    let sink = UUri::try_from_parts(
        SERVICE_AUTHORITY,
        SERVICE_UE_ID,
        SERVICE_UE_VERSION_MAJOR,
        SERVICE_RESOURCE_ID,
    )
    .unwrap();

    let zenoh_config = common::get_zenoh_config();

    let client = UPTransportZenoh::new(zenoh_config, source.clone())
        .await
        .unwrap();

    let wrapped_client: Arc<dyn UTransport> = Arc::new(client);

    // Initiate the listener which will wait for messages in the background.
    // Since we want to listen to responses to the requests we are making we need to flip the sink (destination) and source (origin)
    let service_response_listener: Arc<dyn UListener> = Arc::new(ServiceResponseListener);
    wrapped_client
        .register_listener(&sink, Some(&source), service_response_listener)
        .await?;

    let mut i = 0;
    loop {
        tokio::time::sleep(Duration::from_millis(1000)).await;

        let hello_request = HelloRequest {
            name: format!("ue_client@i={}", i).to_string(),
            ..Default::default()
        };
        i += 1;

        // Build a Request Message addressed to the sink and signed by the source
        let request_msg = UMessageBuilder::request(sink.clone(), source.clone(), REQUEST_TTL)
            .build_with_protobuf_payload(&hello_request)
            .unwrap();
        info!("Sending Request message:\n{request_msg:?}");

        wrapped_client.send(request_msg).await?;
    }
}

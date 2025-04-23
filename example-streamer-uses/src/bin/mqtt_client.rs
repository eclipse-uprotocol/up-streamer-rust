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

use common::ServiceResponseListener;
use hello_world_protos::hello_world_service::HelloRequest;
use log::info;
use std::sync::Arc;
use std::time::Duration;
use up_rust::{UListener, UMessageBuilder, UStatus, UTransport, UUri};
use up_transport_mqtt5::{Mqtt5Transport, Mqtt5TransportOptions, MqttClientOptions};

const SERVICE_AUTHORITY: &str = "authority_B";
const SERVICE_UE_ID: u32 = 0x1236;
const SERVICE_UE_VERSION_MAJOR: u8 = 1;
const SERVICE_RESOURCE_ID: u16 = 0x0421;

const CLIENT_AUTHORITY: &str = "authority_A";
const CLIENT_UE_ID: u32 = 0x4321;
const CLIENT_UE_VERSION_MAJOR: u8 = 1;
const CLIENT_RESOURCE_ID: u16 = 0;

const REQUEST_TTL: u32 = 1000;

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    info!("Started mqtt_client.");

    // Source represents the client (specifically the topic that the client sends to)
    let source = UUri::try_from_parts(
        CLIENT_AUTHORITY,
        CLIENT_UE_ID,
        CLIENT_UE_VERSION_MAJOR,
        CLIENT_RESOURCE_ID,
    )
    .unwrap();
    // Sink is the destination entity which the streamer should rout our messages to.
    let sink = UUri::try_from_parts(
        SERVICE_AUTHORITY,
        SERVICE_UE_ID,
        SERVICE_UE_VERSION_MAJOR,
        SERVICE_RESOURCE_ID,
    )
    .unwrap();

    let mqtt_client_options = MqttClientOptions {
        broker_uri: "localhost:1883".to_string(),
        ..Default::default()
    };
    let mqtt_transport_options = Mqtt5TransportOptions {
        mqtt_client_options,
        ..Default::default()
    };
    let mqtt5_transport =
        Mqtt5Transport::new(mqtt_transport_options, CLIENT_AUTHORITY.to_string()).await?;
    mqtt5_transport.connect().await?;

    let client: Arc<dyn UTransport> = Arc::new(mqtt5_transport);

    let service_response_listener: Arc<dyn UListener> = Arc::new(ServiceResponseListener);
    client
        .register_listener(&sink, Some(&source), service_response_listener)
        .await?;

    let mut i = 0;
    loop {
        tokio::time::sleep(Duration::from_millis(1000)).await;

        let hello_request = HelloRequest {
            name: format!("mqtt_client@i={}", i).to_string(),
            ..Default::default()
        };
        i += 1;

        let request_msg = UMessageBuilder::request(sink.clone(), source.clone(), REQUEST_TTL)
            .build_with_protobuf_payload(&hello_request)
            .unwrap();
        info!("Sending Request message:\n{request_msg:?}");

        client.send(request_msg).await?;
    }
}

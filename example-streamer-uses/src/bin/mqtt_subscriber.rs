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

use common::PublishReceiver;
use log::info;
use std::sync::Arc;
use std::thread;
use up_rust::{UListener, UStatus, UTransport, UUri, UUID};
use up_transport_mqtt5::{MqttConfig, UPClientMqtt, UPClientMqttType};

const PUB_TOPIC_AUTHORITY: &str = "authority_B";
const PUB_TOPIC_UE_ID: u32 = 0x3039;
const PUB_TOPIC_UE_VERSION_MAJOR: u8 = 1;
const PUB_TOPIC_RESOURCE_ID: u16 = 0x8001;

const SUB_TOPIC_AUTHORITY: &str = "authority_A";

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    info!("Started mqtt_subscriber.");

    // Here we define which sources we want to accept messages from
    let source_filter = UUri::try_from_parts(
        PUB_TOPIC_AUTHORITY,
        PUB_TOPIC_UE_ID,
        PUB_TOPIC_UE_VERSION_MAJOR,
        PUB_TOPIC_RESOURCE_ID,
    )
    .unwrap();

    let ssl_options = None;

    let mqtt_config = MqttConfig {
        mqtt_protocol: up_transport_mqtt5::MqttProtocol::Mqtt,
        mqtt_port: 1883,
        mqtt_hostname: "localhost".to_string(),
        max_buffered_messages: 100,
        max_subscriptions: 100,
        session_expiry_interval: 3600,
        ssl_options,
        username: "user".to_string(),
    };

    let subscriber: Arc<dyn UTransport> = Arc::new(
        UPClientMqtt::new(
            mqtt_config,
            UUID::build(),
            SUB_TOPIC_AUTHORITY.to_string(),
            UPClientMqttType::Device,
        )
        .await
        .expect("Could not create mqtt transport."),
    );

    let publish_receiver: Arc<dyn UListener> = Arc::new(PublishReceiver);
    subscriber
        .register_listener(&source_filter, None, publish_receiver.clone())
        .await?;

    thread::park();
    Ok(())
}

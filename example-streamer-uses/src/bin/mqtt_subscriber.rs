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
use hello_world_protos::hello_world_topics::Timer;
use log::{debug, error, info};
use protobuf::Message;
use std::sync::Arc;
use std::thread;
use up_client_mqtt5_rust::{UPClientMqtt, UPClientMqttType};
use up_rust::{UListener, UMessage, UStatus, UTransport, UUri, UUID};

const PUB_TOPIC_AUTHORITY: &str = "ecu_authority";
const PUB_TOPIC_UE_ID: u32 = 0x3039;
const PUB_TOPIC_UE_VERSION_MAJOR: u8 = 1;
const PUB_TOPIC_RESOURCE_ID: u16 = 0x8001;

const SUB_TOPIC_AUTHORITY: &str = "cloud_authority";

#[allow(dead_code)]
struct PublishReceiver;

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

    let mqtt_config = common::get_mqtt_config();

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

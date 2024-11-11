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

use chrono::Local;
use chrono::Timelike;
use hello_world_protos::hello_world_topics::Timer;
use hello_world_protos::timeofday::TimeOfDay;
use log::info;
use std::sync::Arc;
use std::time::Duration;
use up_client_mqtt5_rust::{UPClientMqtt, UPClientMqttType};
use up_rust::{UMessageBuilder, UStatus, UTransport, UUri, UUID};

const PUB_TOPIC_AUTHORITY: &str = "cloud_authority";
const PUB_TOPIC_UE_ID: u32 = 0x3039;
const PUB_TOPIC_UE_VERSION_MAJOR: u8 = 1;
const PUB_TOPIC_RESOURCE_ID: u16 = 0x8001;

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    info!("Started mqtt_publisher.");

    // This is the URI of the publisher entity
    let source = UUri::try_from_parts(
        PUB_TOPIC_AUTHORITY,
        PUB_TOPIC_UE_ID,
        PUB_TOPIC_UE_VERSION_MAJOR,
        PUB_TOPIC_RESOURCE_ID,
    )
    .unwrap();

    let mqtt_config = common::get_mqtt_config();

    let publisher: Arc<dyn UTransport> = Arc::new(
        UPClientMqtt::new(
            mqtt_config,
            UUID::build(),
            PUB_TOPIC_AUTHORITY.to_string(),
            UPClientMqttType::Device,
        )
        .await
        .expect("Could not create mqtt transport."),
    );

    loop {
        tokio::time::sleep(Duration::from_millis(1000)).await;

        let now = Local::now();

        let time_of_day = TimeOfDay {
            hours: now.hour() as i32,
            minutes: now.minute() as i32,
            seconds: now.second() as i32,
            nanos: now.nanosecond() as i32,
            ..Default::default()
        };

        let timer_message = Timer {
            time: Some(time_of_day).into(),
            ..Default::default()
        };

        // Publish messages signed with the source URI
        let publish_msg = UMessageBuilder::publish(source.clone())
            .build_with_protobuf_payload(&timer_message)
            .unwrap();
        info!("Sending Publish message:\n{publish_msg:?}");

        publisher.send(publish_msg).await?;
    }
}

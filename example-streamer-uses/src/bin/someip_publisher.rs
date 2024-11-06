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

use chrono::Local;
use chrono::Timelike;
use hello_world_protos::hello_world_topics::Timer;
use hello_world_protos::timeofday::TimeOfDay;
use log::trace;
use std::fs::canonicalize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use up_rust::{UMessageBuilder, UStatus, UTransport, UUri};
use up_transport_vsomeip::UPTransportVsomeip;

const PUB_TOPIC_AUTHORITY: &str = "me_authority";
const PUB_TOPIC_UE_ID: u32 = 0x5BA0;
const PUB_TOPIC_UE_VERSION_MAJOR: u8 = 1;
const PUB_TOPIC_RESOURCE_ID: u16 = 0x8001;

const REMOTE_AUTHORITY: &str = "linux";

fn publisher_uuri() -> UUri {
    UUri::try_from_parts(
        PUB_TOPIC_AUTHORITY,
        PUB_TOPIC_UE_ID,
        PUB_TOPIC_UE_VERSION_MAJOR,
        0,
    )
    .unwrap()
}

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    println!("mE_publisher");

    let crate_dir = env!("CARGO_MANIFEST_DIR");
    // TODO: Make configurable to pass the path to the vsomeip config as a command line argument
    let vsomeip_config = PathBuf::from(crate_dir).join("vsomeip-configs/mE_publisher.json");
    let vsomeip_config = canonicalize(vsomeip_config).ok();
    trace!("vsomeip_config: {vsomeip_config:?}");

    // There will be a single vsomeip_transport, as there is a connection into device and a streamer
    let publisher: Arc<dyn UTransport> = Arc::new(
        UPTransportVsomeip::new_with_config(
            publisher_uuri(),
            &REMOTE_AUTHORITY.to_string(),
            &vsomeip_config.unwrap(),
            None,
        )
        .unwrap(),
    );

    let source = UUri::try_from_parts(
        PUB_TOPIC_AUTHORITY,
        PUB_TOPIC_UE_ID,
        PUB_TOPIC_UE_VERSION_MAJOR,
        PUB_TOPIC_RESOURCE_ID,
    )
    .unwrap();

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

        let publish_msg = UMessageBuilder::publish(source.clone())
            .build_with_protobuf_payload(&timer_message)
            .unwrap();
        println!("Sending Publish message:\n{publish_msg:?}");

        publisher.send(publish_msg).await?;
    }
}

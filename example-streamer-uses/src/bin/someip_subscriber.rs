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
use hello_world_protos::hello_world_topics::Timer;
use log::{error, trace};
use protobuf::Message;
use std::fs::canonicalize;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use up_rust::{UListener, UMessage, UStatus, UTransport, UUri};
use up_transport_vsomeip::UPTransportVsomeip;

const SUBSCRIBER_AUTHORITY: &str = "me_authority";
const SUBSCRIBER_UE_ID: u32 = 0x1236;
const SUBSCRIBER_UE_VERSION_MAJOR: u8 = 1;

const PUB_TOPIC_AUTHORITY: &str = "linux";
const PUB_TOPIC_UE_ID: u32 = 0x3039;
const PUB_TOPIC_UE_VERSION_MAJOR: u8 = 1;
const PUB_TOPIC_RESOURCE_ID: u16 = 0x8001;

fn subscriber_uuri() -> UUri {
    UUri::try_from_parts(
        SUBSCRIBER_AUTHORITY,
        SUBSCRIBER_UE_ID,
        SUBSCRIBER_UE_VERSION_MAJOR,
        0,
    )
    .unwrap()
}

#[allow(dead_code)]
struct PublishReceiver;

#[async_trait]
impl UListener for PublishReceiver {
    async fn on_receive(&self, msg: UMessage) {
        println!("PublishReceiver: Received a message: {msg:?}");

        let Some(payload_bytes) = msg.payload else {
            panic!("No bytes available");
        };
        match Timer::parse_from_bytes(&payload_bytes) {
            Ok(timer_message) => {
                println!("timer: {timer_message:?}");
                timer_message
            }
            Err(err) => {
                error!("Unable to parse Timer Message: {err:?}");
                return;
            }
        };
    }
}

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    println!("mE_subscriber");

    let crate_dir = env!("CARGO_MANIFEST_DIR");
    // TODO: Make configurable to pass the path to the vsomeip config as a command line argument
    let vsomeip_config = PathBuf::from(crate_dir).join("vsomeip-configs/mE_subscriber.json");
    let vsomeip_config = canonicalize(vsomeip_config).ok();
    trace!("vsomeip_config: {vsomeip_config:?}");

    // There will be a single vsomeip_transport, as there is a connection into device and a streamer
    // TODO: Add error handling if we fail to create a UPTransportVsomeip
    let subscriber: Arc<dyn UTransport> = Arc::new(
        UPTransportVsomeip::new_with_config(
            subscriber_uuri(),
            &PUB_TOPIC_AUTHORITY.to_string(),
            &vsomeip_config.unwrap(),
            None,
        )
        .unwrap(),
    );

    let source_filter = UUri::try_from_parts(
        PUB_TOPIC_AUTHORITY,
        PUB_TOPIC_UE_ID,
        PUB_TOPIC_UE_VERSION_MAJOR,
        PUB_TOPIC_RESOURCE_ID,
    )
    .unwrap();

    let publish_receiver: Arc<dyn UListener> = Arc::new(PublishReceiver);
    // TODO: Need to revisit how the vsomeip config file is used in non point-to-point cases
    subscriber
        .register_listener(&source_filter, None, publish_receiver.clone())
        .await?;

    thread::park();
    Ok(())
}

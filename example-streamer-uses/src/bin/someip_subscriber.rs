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
use log::trace;
use std::fs::canonicalize;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use up_rust::{UListener, UStatus, UTransport, UUri};
use up_transport_vsomeip::UPTransportVsomeip;

const SUB_TOPIC_AUTHORITY: &str = "authority_B";
const SUB_TOPIC_UE_ID: u32 = 0x5BB0;
const SUB_TOPIC_UE_VERSION_MAJOR: u8 = 1;

const PUB_TOPIC_AUTHORITY: &str = "authority_B";
const PUB_TOPIC_UE_ID: u32 = 0x3039;
const PUB_TOPIC_UE_VERSION_MAJOR: u8 = 1;
const PUB_TOPIC_RESOURCE_ID: u16 = 0x8001;

fn subscriber_uuri() -> UUri {
    UUri::try_from_parts(
        SUB_TOPIC_AUTHORITY,
        SUB_TOPIC_UE_ID,
        SUB_TOPIC_UE_VERSION_MAJOR,
        0,
    )
    .unwrap()
}

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    println!("mE_subscriber");

    let crate_dir = env!("CARGO_MANIFEST_DIR");
    // TODO: Make configurable to pass the path to the vsomeip config as a command line argument
    let vsomeip_config = PathBuf::from(crate_dir).join("vsomeip-configs/someip_subscriber.json");
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

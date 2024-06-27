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
use hello_world_protos::hello_world_topics::Timer;
use log::error;
use protobuf::Message;
use std::str::FromStr;
use std::sync::Arc;
use std::thread;
use up_rust::{UListener, UMessage, UStatus, UTransport, UUri};
use up_transport_zenoh::UPClientZenoh;
use zenoh::config::{Config, EndPoint};

const PUB_TOPIC_AUTHORITY: &str = "me_authority";
const PUB_TOPIC_UE_ID: u16 = 0x5BA0;
const PUB_TOPIC_UE_VERSION_MAJOR: u8 = 1;
const PUB_TOPIC_RESOURCE_ID: u16 = 0x8001;

#[allow(dead_code)]
struct PublishReceiver;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The endpoint for Zenoh client to connect to
    #[arg(short, long, default_value = "tcp/0.0.0.0:7442")]
    endpoint: String,
}

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
            }
            Err(err) => {
                error!("Unable to parse Timer Message: {err:?}");
            }
        };
    }

    async fn on_error(&self, err: UStatus) {
        println!("ServiceRequestResponder: Encountered an error: {err:?}");
    }
}

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    let args = Args::parse();

    println!("uE_subscriber");

    // TODO: Probably make somewhat configurable?
    // Create a configuration object
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
    let subscriber: Arc<dyn UTransport> = Arc::new(
        UPClientZenoh::new(zenoh_config, "linux".to_string())
            .await
            .unwrap(),
    );

    let source_filter = UUri {
        authority_name: PUB_TOPIC_AUTHORITY.to_string(),
        ue_id: PUB_TOPIC_UE_ID as u32,
        ue_version_major: PUB_TOPIC_UE_VERSION_MAJOR as u32,
        resource_id: PUB_TOPIC_RESOURCE_ID as u32,
        ..Default::default()
    };

    let publish_receiver: Arc<dyn UListener> = Arc::new(PublishReceiver);
    // TODO: Need to revisit how the vsomeip config file is used in non point-to-point cases
    subscriber
        .register_listener(&source_filter, None, publish_receiver.clone())
        .await?;

    thread::park();
    Ok(())
}

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
use clap::Parser;
use hello_world_protos::hello_world_topics::Timer;
use hello_world_protos::timeofday::TimeOfDay;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use up_rust::{UMessageBuilder, UStatus, UTransport, UUri};
use up_transport_zenoh::UPClientZenoh;
use zenoh::config::{Config, EndPoint};

const PUB_TOPIC_AUTHORITY: &str = "linux";
const PUB_TOPIC_UE_ID: u16 = 0x3039;
const PUB_TOPIC_UE_VERSION_MAJOR: u8 = 1;
const PUB_TOPIC_RESOURCE_ID: u16 = 0x8001;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The endpoint for Zenoh client to connect to
    #[arg(short, long, default_value = "tcp/0.0.0.0:7444")]
    endpoint: String,
}

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    let args = Args::parse();

    println!("uE_publisher");

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
    let publisher: Arc<dyn UTransport> = Arc::new(
        UPClientZenoh::new(zenoh_config, "linux".to_string())
            .await
            .unwrap(),
    );

    let source = UUri {
        authority_name: PUB_TOPIC_AUTHORITY.to_string(),
        ue_id: PUB_TOPIC_UE_ID as u32,
        ue_version_major: PUB_TOPIC_UE_VERSION_MAJOR as u32,
        resource_id: PUB_TOPIC_RESOURCE_ID as u32,
        ..Default::default()
    };

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

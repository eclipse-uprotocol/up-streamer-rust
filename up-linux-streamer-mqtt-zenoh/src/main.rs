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

mod config;

use crate::config::Config;
use clap::Parser;
use log::{error, trace};
use std::fs::File;
use std::io::Read;
use std::str::FromStr;
use std::sync::Arc;
use std::thread;
use up_client_mqtt5_rust::{MqttConfig, MqttProtocol, UPClientMqtt, UPClientMqttType};
use up_rust::{UCode, UStatus, UTransport, UUri, UUID};
use up_streamer::{Endpoint, UStreamer};
use up_transport_zenoh::UPTransportZenoh;
use usubscription_static_file::USubscriptionStaticFile;
use zenoh::config::Config as ZenohConfig;

const SERVICE_AUTHORITY: &str = "zenoh_authority";
const SERVICE_UE_ID: u32 = 0x4444;
const SERVICE_UE_VERSION_MAJOR: u8 = 1;

fn streamer_uuri() -> UUri {
    UUri::try_from_parts(
        SERVICE_AUTHORITY,
        SERVICE_UE_ID,
        SERVICE_UE_VERSION_MAJOR,
        0,
    )
    .unwrap()
}

#[derive(Parser)]
#[command()]
struct StreamerArgs {
    #[arg(short, long, value_name = "FILE")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    let args = StreamerArgs::parse();

    let mut file = File::open(args.config)
        .map_err(|e| UStatus::fail_with_code(UCode::NOT_FOUND, format!("File not found: {e:?}")))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).map_err(|e| {
        UStatus::fail_with_code(
            UCode::INTERNAL,
            format!("Unable to read config file: {e:?}"),
        )
    })?;

    let config: Config = json5::from_str(&contents).map_err(|e| {
        UStatus::fail_with_code(
            UCode::INTERNAL,
            format!("Unable to parse config file: {e:?}"),
        )
    })?;

    let subscription_path = config.usubscription_config.file_path;
    let usubscription = Arc::new(USubscriptionStaticFile::new(subscription_path));

    let mut streamer = UStreamer::new(
        "up-linux-streamer",
        config.up_streamer_config.message_queue_size,
        usubscription,
    )
    .expect("Failed to create uStreamer");

    let zenoh_config =
        ZenohConfig::from_file("up-linux-streamer-mqtt-zenoh/ZENOH_CONFIG.json5").unwrap();

    let streamer_uri: String = (&streamer_uuri()).into();

    let zenoh_transport: Arc<dyn UTransport> = Arc::new(
        UPTransportZenoh::new(zenoh_config, streamer_uri)
            .await
            .expect("Unable to initialize Zenoh UTransport"),
    );

    let zenoh_endpoint = Endpoint::new("host_endpoint", "zenoh_authority", zenoh_transport.clone());

    let mqtt_config = MqttConfig {
        mqtt_protocol: MqttProtocol::Mqtt,
        mqtt_hostname: "localhost".to_string(),
        mqtt_port: 1883,
        max_buffered_messages: 100,
        max_subscriptions: 100,
        session_expiry_interval: 3600,
        ssl_options: None,
        username: "user_name".to_string(),
    };

    let mqtt_transport: Arc<dyn UTransport> = Arc::new(
        UPClientMqtt::new(
            mqtt_config,
            UUID::build(),
            "cloud".to_string(),
            UPClientMqttType::Device,
        )
        .await
        .expect("Could not create mqtt transport."),
    );

    let mqtt_endpoint = Endpoint::new("mqtt endpoint", "linux", mqtt_transport.clone());

    streamer
        .add_forwarding_rule(zenoh_endpoint.clone(), mqtt_endpoint.clone())
        .await
        .expect("Could not add zenoh -> mqtt forwarding rule");

    streamer
        .add_forwarding_rule(mqtt_endpoint.clone(), zenoh_endpoint.clone())
        .await
        .expect("Could not add mqtt -> zenoh forwarding rule");

    thread::park();

    Ok(())
}

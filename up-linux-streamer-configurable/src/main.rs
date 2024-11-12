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
use log::info;
use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;
use std::thread;
use std::{fmt::format, fs::File};
use up_client_mqtt5_rust::{MqttConfig, MqttProtocol, UPClientMqtt, UPClientMqttType};
use up_rust::{UCode, UStatus, UTransport, UUri, UUID};
use up_streamer::{Endpoint, UStreamer};
use up_transport_vsomeip::UPTransportVsomeip;
use up_transport_zenoh::UPTransportZenoh;
use usubscription_static_file::USubscriptionStaticFile;
use zenoh::config::Config as ZenohConfig;

const SERVICE_AUTHORITY: &str = "ecu_authority";
const SERVICE_UE_ID: u32 = 0x4444;
const SERVICE_UE_VERSION_MAJOR: u8 = 1;
const SERVICE_RESOURCE_ID: u16 = 0;

#[derive(Parser)]
#[command()]
struct StreamerArgs {
    #[arg(short, long, value_name = "FILE")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    info!("Started up-linux-streamer-mqtt-zenoh");

    // Get the config file.
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

    // Create a vector with an endpoint for each enabled transport
    let mut endpoints: HashMap<config::Transport, Endpoint> = HashMap::new();
    for transport in config.enabled_transports.clone() {
        endpoints.insert(
            transport.clone(),
            build_endpoint(transport, config.clone()).await,
        );
    }

    let subscription_path = config.usubscription_config.file_path;
    let usubscription = Arc::new(USubscriptionStaticFile::new(subscription_path));

    // Start the streamer instance.
    let mut streamer = UStreamer::new(
        "streamer",
        config.up_streamer_config.message_queue_size,
        usubscription,
    )
    .expect("Failed to create uStreamer");

    if config.forwarding.contains(&config::Forwarding::MqttToZenoh) {
        streamer
            .add_forwarding_rule(
                endpoints.get(&config::Transport::Mqtt).unwrap().to_owned(),
                endpoints.get(&config::Transport::Zenoh).unwrap().to_owned(),
            )
            .await
            .expect("Could not add mqtt -> zenoh forwarding rule");
    };
    if config
        .forwarding
        .contains(&config::Forwarding::MqttToSomeip)
    {
        streamer
            .add_forwarding_rule(
                endpoints.get(&config::Transport::Mqtt).unwrap().to_owned(),
                endpoints
                    .get(&config::Transport::Someip)
                    .unwrap()
                    .to_owned(),
            )
            .await
            .expect("Could not add mqtt -> zenoh forwarding rule");
    };
    if config.forwarding.contains(&config::Forwarding::ZenohToMqtt) {
        streamer
            .add_forwarding_rule(
                endpoints.get(&config::Transport::Zenoh).unwrap().to_owned(),
                endpoints.get(&config::Transport::Mqtt).unwrap().to_owned(),
            )
            .await
            .expect("Could not add mqtt -> zenoh forwarding rule");
    };
    if config
        .forwarding
        .contains(&config::Forwarding::ZenohToSomeip)
    {
        streamer
            .add_forwarding_rule(
                endpoints.get(&config::Transport::Zenoh).unwrap().to_owned(),
                endpoints
                    .get(&config::Transport::Someip)
                    .unwrap()
                    .to_owned(),
            )
            .await
            .expect("Could not add mqtt -> zenoh forwarding rule");
    };
    if config
        .forwarding
        .contains(&config::Forwarding::SomeipToMqtt)
    {
        streamer
            .add_forwarding_rule(
                endpoints
                    .get(&config::Transport::Someip)
                    .unwrap()
                    .to_owned(),
                endpoints.get(&config::Transport::Mqtt).unwrap().to_owned(),
            )
            .await
            .expect("Could not add mqtt -> zenoh forwarding rule");
    };
    if config
        .forwarding
        .contains(&config::Forwarding::SomeipToZenoh)
    {
        streamer
            .add_forwarding_rule(
                endpoints
                    .get(&config::Transport::Someip)
                    .unwrap()
                    .to_owned(),
                endpoints.get(&config::Transport::Zenoh).unwrap().to_owned(),
            )
            .await
            .expect("Could not add mqtt -> zenoh forwarding rule");
    };
    thread::park();

    Ok(())
}

async fn build_endpoint(transport: config::Transport, config: config::Config) -> Endpoint {
    return match transport {
        config::Transport::Mqtt => {
            let mqtt_config = MqttConfig {
                mqtt_protocol: MqttProtocol::Mqtt,
                mqtt_hostname: config.mqtt_config.mqtt_hostname,
                mqtt_port: config.mqtt_config.mqtt_port,
                max_buffered_messages: config.mqtt_config.max_buffered_messages,
                max_subscriptions: config.mqtt_config.max_subscriptions,
                session_expiry_interval: config.mqtt_config.session_expiry_interval,
                ssl_options: None,
                username: config.mqtt_config.username,
            };
            let transport: Arc<dyn UTransport> = Arc::new(
                UPClientMqtt::new(
                    mqtt_config,
                    UUID::build(),
                    config.mqtt_config.authority.clone(),
                    UPClientMqttType::Device,
                )
                .await
                .expect("Could not create mqtt transport."),
            );
            Endpoint::new(
                &config.mqtt_config.endpoint_name,
                &config.mqtt_config.authority,
                transport,
            )
        }
        config::Transport::Zenoh => {
            let streamer_uri = UUri::try_from_parts(
                &config.streamer_uuri.authority,
                config.streamer_uuri.ue_id,
                config.streamer_uuri.ue_version_major,
                config.streamer_uuri.resource_id,
            )
            .unwrap();
            let zenoh_config = ZenohConfig::from_file(config.zenoh_config.config_file).unwrap();
            let transport: Arc<dyn UTransport> = Arc::new(
                UPTransportZenoh::new(zenoh_config, streamer_uri)
                    .await
                    .expect("Unable to initialize Zenoh UTransport"),
            );
            Endpoint::new(
                &config.zenoh_config.endpoint_name,
                &config.zenoh_config.authority,
                transport,
            )
        }
        config::Transport::Someip => {
            let streamer_uri = UUri::try_from_parts(
                &config.streamer_uuri.authority,
                config.streamer_uuri.ue_id,
                config.streamer_uuri.ue_version_major,
                config.streamer_uuri.resource_id,
            )
            .unwrap();

            let transport: Arc<dyn UTransport> = Arc::new(
                UPTransportVsomeip::new_with_config(
                    streamer_uri,
                    &config.someip_config.authority,
                    &config.someip_config.config_file,
                    None,
                )
                .expect("Unable to initialize vsomeip UTransport"),
            );
            Endpoint::new(
                &config.someip_config.endpoint_name,
                &config.someip_config.authority,
                transport,
            )
        }
    };
}

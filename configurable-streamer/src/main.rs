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
use std::io::Read;
use std::sync::Arc;
use std::thread;
use std::{collections::HashMap, fs::File};
use tracing::info;
use up_rust::{UCode, UStatus, UTransport};
use up_streamer::{Endpoint, UStreamer};
use up_transport_mqtt5::{Mqtt5Transport, Mqtt5TransportOptions, MqttClientOptions};
use up_transport_zenoh::{zenoh_config::Config as ZenohConfig, UPTransportZenoh};
use usubscription_static_file::USubscriptionStaticFile;

#[derive(Parser)]
#[command()]
struct StreamerArgs {
    #[arg(short, long, value_name = "FILE")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    let _ = tracing_subscriber::fmt::try_init();

    info!("Started up-linux-streamer-configurable");

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

    let mut config: Config = json5::from_str(&contents).map_err(|e| {
        UStatus::fail_with_code(
            UCode::INTERNAL,
            format!("Unable to parse config file: {e:?}"),
        )
    })?;
    config.transports.mqtt.load_mqtt_details().unwrap();

    let subscription_path = config.usubscription_config.file_path;
    let usubscription = Arc::new(USubscriptionStaticFile::new(subscription_path));

    // Start the streamer instance.
    let mut streamer = UStreamer::new(
        "up-streamer",
        config.up_streamer_config.message_queue_size,
        usubscription,
    )
    .expect("Failed to create uStreamer");

    let mut endpoints: HashMap<String, Endpoint> = HashMap::new();

    // build the zenoh transport
    let zenoh_config = ZenohConfig::from_file(config.transports.zenoh.config_file).unwrap();
    let zenoh_transport: Arc<dyn UTransport> = Arc::new(
        UPTransportZenoh::builder(config.streamer_uuri.authority.clone())
            .expect("Unable to create Zenoh transport builder")
            .with_config(zenoh_config)
            .build()
            .await
            .expect("Unable to initialize Zenoh UTransport"),
    );

    // build the mqtt5 transport
    let mqtt_client_options = MqttClientOptions {
        broker_uri: config
            .transports
            .mqtt
            .mqtt_details
            .clone()
            .unwrap()
            .hostname
            + ":"
            + &config
                .transports
                .mqtt
                .mqtt_details
                .unwrap()
                .port
                .to_string(),
        ..Default::default()
    };
    let mqtt_transport_options = Mqtt5TransportOptions {
        mqtt_client_options,
        ..Default::default()
    };
    let mqtt5_transport = Mqtt5Transport::new(
        mqtt_transport_options,
        config.streamer_uuri.authority.clone(),
    )
    .await?;
    mqtt5_transport.connect().await?;
    let mqtt5_transport: Arc<dyn UTransport> = Arc::new(mqtt5_transport);

    // build all zenoh endpoints
    for zenoh_endpoint_config in config.transports.zenoh.endpoints.clone() {
        let endpoint = Endpoint::new(
            &zenoh_endpoint_config.endpoint,
            &zenoh_endpoint_config.authority,
            zenoh_transport.clone(),
        );
        if endpoints
            .insert(zenoh_endpoint_config.endpoint.clone(), endpoint)
            .is_some()
        {
            return Err(UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                format!(
                    "Duplicate endpoint name found: {}",
                    zenoh_endpoint_config.endpoint
                ),
            ));
        }
    }

    // build all mqtt endpoints
    for mqtt_endpoint_config in config.transports.mqtt.endpoints.clone() {
        let endpoint = Endpoint::new(
            &mqtt_endpoint_config.endpoint,
            &mqtt_endpoint_config.authority,
            mqtt5_transport.clone(),
        );
        if endpoints
            .insert(mqtt_endpoint_config.endpoint.clone(), endpoint)
            .is_some()
        {
            return Err(UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                format!(
                    "Duplicate endpoint name found: {}",
                    mqtt_endpoint_config.endpoint
                ),
            ));
        }
    }

    // set up the endpoint forwarding for zenoh
    for zenoh_endpoint in config.transports.zenoh.endpoints {
        for forwarding in zenoh_endpoint.forwarding {
            let left_endpoint = endpoints.get(&zenoh_endpoint.endpoint).unwrap();
            let right_endpoint = endpoints.get(&forwarding).unwrap();
            streamer
                .add_forwarding_rule(left_endpoint.to_owned(), right_endpoint.to_owned())
                .await
                .expect("Could not add forwarding rule from {zenoh.endpoint} to {forwarding}");
        }
    }

    // set up the endpoint forwarding for mqtt
    for mqtt5_endpoint in config.transports.mqtt.endpoints {
        for forwarding in mqtt5_endpoint.forwarding {
            let left_endpoint = endpoints.get(&mqtt5_endpoint.endpoint).unwrap();
            let right_endpoint = endpoints.get(&forwarding).unwrap();
            streamer
                .add_forwarding_rule(left_endpoint.to_owned(), right_endpoint.to_owned())
                .await
                .expect("Could not add forwarding rule from {mqtt.endpoint} to {forwarding}");
        }
    }

    thread::park();

    Ok(())
}

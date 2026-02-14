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

use crate::config::{Config, EndpointConfig, SubscriptionProviderMode};
use clap::Parser;
use std::io::Read;
use std::sync::Arc;
use std::{collections::HashMap, fs::File};
use tracing::info;
use up_rust::core::usubscription::USubscription;
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

fn register_transport_endpoints(
    endpoints: &mut HashMap<String, Endpoint>,
    endpoint_configs: &[EndpointConfig],
    transport: Arc<dyn UTransport>,
) -> Result<(), UStatus> {
    for endpoint_config in endpoint_configs {
        let endpoint = Endpoint::new(
            &endpoint_config.endpoint,
            &endpoint_config.authority,
            transport.clone(),
        );

        if endpoints
            .insert(endpoint_config.endpoint.clone(), endpoint)
            .is_some()
        {
            return Err(UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                format!(
                    "Duplicate endpoint name found: {}",
                    endpoint_config.endpoint
                ),
            ));
        }
    }

    Ok(())
}

async fn wire_forwarding_rules(
    streamer: &mut UStreamer,
    endpoints: &HashMap<String, Endpoint>,
    endpoint_configs: &[EndpointConfig],
) -> Result<(), UStatus> {
    for endpoint_config in endpoint_configs {
        for forwarding_target in &endpoint_config.forwarding {
            let left_endpoint = endpoints.get(&endpoint_config.endpoint).ok_or_else(|| {
                UStatus::fail_with_code(
                    UCode::INVALID_ARGUMENT,
                    format!(
                        "Unknown endpoint in forwarding rules: {}",
                        endpoint_config.endpoint
                    ),
                )
            })?;
            let right_endpoint = endpoints.get(forwarding_target).ok_or_else(|| {
                UStatus::fail_with_code(
                    UCode::INVALID_ARGUMENT,
                    format!("Unknown forwarding target endpoint: {forwarding_target}"),
                )
            })?;

            streamer
                .add_route_ref(left_endpoint, right_endpoint)
                .await?;
        }
    }

    Ok(())
}

async fn wait_for_shutdown_signal() -> Result<(), UStatus> {
    tokio::signal::ctrl_c().await.map_err(|error| {
        UStatus::fail_with_code(
            UCode::INTERNAL,
            format!("Unable to wait for shutdown signal: {error:?}"),
        )
    })
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
    config.transports.mqtt.load_mqtt_details().map_err(|e| {
        UStatus::fail_with_code(
            UCode::INVALID_ARGUMENT,
            format!("Unable to load MQTT transport details: {e:?}"),
        )
    })?;

    let usubscription: Arc<dyn USubscription> = match config.usubscription_config.mode {
        SubscriptionProviderMode::StaticFile => Arc::new(USubscriptionStaticFile::new(
            config.usubscription_config.file_path.clone(),
        )),
        SubscriptionProviderMode::LiveUsubscription => {
            return Err(UStatus::fail_with_code(
                    UCode::UNIMPLEMENTED,
                    "live_usubscription mode is reserved in this phase; live runtime integration is deferred (see reports/usubscription-decoupled-pubsub-migration/05-live-integration-deferred.md)",
                ));
        }
    };

    // Start the streamer instance.
    let mut streamer = UStreamer::new(
        "up-streamer",
        config.up_streamer_config.message_queue_size,
        usubscription,
    )
    .await?;

    let mut endpoints: HashMap<String, Endpoint> = HashMap::new();

    // build the zenoh transport
    let zenoh_config =
        ZenohConfig::from_file(config.transports.zenoh.config_file).map_err(|e| {
            UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                format!("Unable to load Zenoh config file: {e:?}"),
            )
        })?;
    let zenoh_transport: Arc<dyn UTransport> = Arc::new(
        UPTransportZenoh::builder(config.streamer_uuri.authority.clone())
            .map_err(|e| {
                UStatus::fail_with_code(
                    UCode::INTERNAL,
                    format!("Unable to create Zenoh transport builder: {e:?}"),
                )
            })?
            .with_config(zenoh_config)
            .build()
            .await
            .map_err(|e| {
                UStatus::fail_with_code(
                    UCode::INTERNAL,
                    format!("Unable to initialize Zenoh UTransport: {e:?}"),
                )
            })?,
    );

    // build the mqtt5 transport
    let mqtt_details = config.transports.mqtt.mqtt_details.clone().ok_or_else(|| {
        UStatus::fail_with_code(
            UCode::INVALID_ARGUMENT,
            "MQTT transport details are missing after load_mqtt_details",
        )
    })?;
    let mqtt_client_options = MqttClientOptions {
        broker_uri: format!("{}:{}", mqtt_details.hostname, mqtt_details.port),
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

    register_transport_endpoints(
        &mut endpoints,
        &config.transports.zenoh.endpoints,
        zenoh_transport,
    )?;
    register_transport_endpoints(
        &mut endpoints,
        &config.transports.mqtt.endpoints,
        mqtt5_transport,
    )?;

    wire_forwarding_rules(
        &mut streamer,
        &endpoints,
        &config.transports.zenoh.endpoints,
    )
    .await?;
    wire_forwarding_rules(&mut streamer, &endpoints, &config.transports.mqtt.endpoints).await?;

    println!("READY streamer_initialized");
    info!("Streamer initialized; waiting for shutdown signal");
    wait_for_shutdown_signal().await?;
    info!("Shutdown signal received; exiting");

    Ok(())
}

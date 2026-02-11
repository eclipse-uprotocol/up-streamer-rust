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

use clap::Parser;
use common::cli;
use common::ServiceRequestResponder;
use std::sync::Arc;
use std::thread;
use tracing::info;
use up_rust::{UListener, UStatus, UTransport, UUri};
use up_transport_mqtt5::{Mqtt5Transport, Mqtt5TransportOptions, MqttClientOptions};

const DEFAULT_UAUTHORITY: &str = "authority-a";
const DEFAULT_UENTITY: &str = "0x4321";
const DEFAULT_UVERSION: &str = "0x1";
const DEFAULT_RESOURCE: &str = "0x0421";
const DEFAULT_BROKER_URI: &str = "localhost:1883";

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// Authority for the local service identity
    #[arg(long, default_value = DEFAULT_UAUTHORITY)]
    uauthority: String,
    /// UEntity ID for local service identity (decimal or 0x-prefixed hex)
    #[arg(long, default_value = DEFAULT_UENTITY)]
    uentity: String,
    /// UEntity major version for local service identity (decimal or 0x-prefixed hex)
    #[arg(long, default_value = DEFAULT_UVERSION)]
    uversion: String,
    /// Resource ID for local service identity (decimal or 0x-prefixed hex)
    #[arg(long, default_value = DEFAULT_RESOURCE)]
    resource: String,
    /// MQTT broker URI in host:port format
    #[arg(long, default_value = DEFAULT_BROKER_URI)]
    broker_uri: String,
}

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    let _ = tracing_subscriber::fmt::try_init();

    let args = Args::parse();

    info!("Started mqtt_service.");

    let uentity = cli::parse_u32_status("--uentity", &args.uentity)?;
    let uversion = cli::parse_u8_status("--uversion", &args.uversion)?;
    let resource = cli::parse_u16_status("--resource", &args.resource)?;

    // We set the source filter to "any" so that we process messages from all device that send some.
    let source_filter = UUri::any();
    // The sink filter gets specified so that we only process messages directed at this entity.
    let sink_filter = cli::build_uuri(&args.uauthority, uentity, uversion, resource)?;

    let mqtt_client_options = MqttClientOptions {
        broker_uri: args.broker_uri,
        ..Default::default()
    };
    let mqtt_transport_options = Mqtt5TransportOptions {
        mqtt_client_options,
        ..Default::default()
    };
    let mqtt5_transport =
        Mqtt5Transport::new(mqtt_transport_options, args.uauthority.to_string()).await?;
    mqtt5_transport.connect().await?;

    let service: Arc<dyn UTransport> = Arc::new(mqtt5_transport);

    let service_request_responder: Arc<dyn UListener> =
        Arc::new(ServiceRequestResponder::new(service.clone()));
    service
        .register_listener(
            &source_filter,
            Some(&sink_filter),
            service_request_responder.clone(),
        )
        .await?;

    println!("READY listener_registered");

    thread::park();
    Ok(())
}

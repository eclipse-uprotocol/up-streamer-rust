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
use common::ServiceRequestResponder;
use log::info;
use std::str::FromStr;
use std::sync::Arc;
use std::thread;
use up_rust::{UListener, UStatus, UTransport, UUri};
use up_transport_zenoh::UPTransportZenoh;
use zenoh::config::{Config, EndPoint};

const SERVICE_AUTHORITY: &str = "authority_B";
const SERVICE_UE_ID: u32 = 0x1236;
const SERVICE_UE_VERSION_MAJOR: u8 = 1;
const SERVICE_RESOURCE_ID: u16 = 0x0421;

fn service_uuri() -> UUri {
    UUri::try_from_parts(
        SERVICE_AUTHORITY,
        SERVICE_UE_ID,
        SERVICE_UE_VERSION_MAJOR,
        0,
    )
    .unwrap()
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The endpoint for Zenoh client to connect to
    #[arg(short, long, default_value = "tcp/0.0.0.0:7443")]
    endpoint: String,
}

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    let args = Args::parse();

    info!("Started zenoh_service");

    let mut zenoh_config = Config::default();

    if !args.endpoint.is_empty() {
        // Specify the address to listen on using IPv4
        let ipv4_endpoint =
            EndPoint::from_str(args.endpoint.as_str()).expect("Unable to set endpoint");

        // Add the IPv4 endpoint to the Zenoh configuration
        zenoh_config
            .listen
            .endpoints
            .set(vec![ipv4_endpoint])
            .expect("Unable to set Zenoh Config");
    }

    let service_uri: String = (&service_uuri()).into();
    let service: Arc<dyn UTransport> = Arc::new(
        UPTransportZenoh::new(zenoh_config, service_uri)
            .await
            .unwrap(),
    );

    let source_filter = UUri::any();
    let sink_filter = UUri::try_from_parts(
        SERVICE_AUTHORITY,
        SERVICE_UE_ID,
        SERVICE_UE_VERSION_MAJOR,
        SERVICE_RESOURCE_ID,
    )
    .unwrap();

    let service_request_responder: Arc<dyn UListener> =
        Arc::new(ServiceRequestResponder::new(service.clone()));
    service
        .register_listener(
            &source_filter,
            Some(&sink_filter),
            service_request_responder.clone(),
        )
        .await?;

    thread::park();
    Ok(())
}

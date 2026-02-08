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
use common::ServiceResponseListener;
use hello_world_protos::hello_world_service::HelloRequest;
use log::{debug, info};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use up_rust::{UListener, UMessageBuilder, UStatus, UTransport};
use up_transport_zenoh::{
    zenoh_config::{Config, EndPoint},
    UPTransportZenoh,
};

const DEFAULT_ENDPOINT: &str = "tcp/127.0.0.1:7447";
const DEFAULT_UAUTHORITY: &str = "authority-b";
const DEFAULT_UENTITY: &str = "0x1236";
const DEFAULT_UVERSION: &str = "0x1";
const DEFAULT_RESOURCE: &str = "0x0";
const DEFAULT_TARGET_AUTHORITY: &str = "authority-a";
const DEFAULT_TARGET_UENTITY: &str = "0x4321";
const DEFAULT_TARGET_UVERSION: &str = "0x1";
const DEFAULT_TARGET_RESOURCE: &str = "0x0421";

const REQUEST_TTL: u32 = 1000;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The endpoint for Zenoh client to connect to
    #[arg(short, long, default_value = DEFAULT_ENDPOINT)]
    endpoint: String,
    /// Authority for the local client identity
    #[arg(long, default_value = DEFAULT_UAUTHORITY)]
    uauthority: String,
    /// UEntity ID for local client identity (decimal or 0x-prefixed hex)
    #[arg(long, default_value = DEFAULT_UENTITY)]
    uentity: String,
    /// UEntity major version for local client identity (decimal or 0x-prefixed hex)
    #[arg(long, default_value = DEFAULT_UVERSION)]
    uversion: String,
    /// Resource ID for local client identity (decimal or 0x-prefixed hex)
    #[arg(long, default_value = DEFAULT_RESOURCE)]
    resource: String,
    /// Authority for the target service URI
    #[arg(long, default_value = DEFAULT_TARGET_AUTHORITY)]
    target_authority: String,
    /// UEntity ID for target service URI (decimal or 0x-prefixed hex)
    #[arg(long, default_value = DEFAULT_TARGET_UENTITY)]
    target_uentity: String,
    /// UEntity major version for target service URI (decimal or 0x-prefixed hex)
    #[arg(long, default_value = DEFAULT_TARGET_UVERSION)]
    target_uversion: String,
    /// Resource ID for target service URI (decimal or 0x-prefixed hex)
    #[arg(long, default_value = DEFAULT_TARGET_RESOURCE)]
    target_resource: String,
}

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    let args = Args::parse();

    let uentity = cli::parse_u32_status("--uentity", &args.uentity)?;
    let uversion = cli::parse_u8_status("--uversion", &args.uversion)?;
    let resource = cli::parse_u16_status("--resource", &args.resource)?;
    let target_uentity = cli::parse_u32_status("--target-uentity", &args.target_uentity)?;
    let target_uversion = cli::parse_u8_status("--target-uversion", &args.target_uversion)?;
    let target_resource = cli::parse_u16_status("--target-resource", &args.target_resource)?;

    info!("Started zenoh_client");

    let mut zenoh_config = Config::default();

    if !args.endpoint.is_empty() {
        // Specify the address to listen on using IPv4
        let ipv4_endpoint =
            EndPoint::from_str(args.endpoint.as_str()).expect("Unable to set endpoint");

        // Add the IPv4 endpoint to the Zenoh configuration
        zenoh_config
            .connect
            .endpoints
            .set(vec![ipv4_endpoint])
            .expect("Unable to set Zenoh Config");
    }

    let source = cli::build_uuri(&args.uauthority, uentity, uversion, resource)?;
    let client: Arc<dyn UTransport> = Arc::new(
        UPTransportZenoh::builder(source.authority_name())
            .expect("Unable to create Zenoh transport builder")
            .with_config(zenoh_config)
            .build()
            .await
            .unwrap(),
    );
    let sink = cli::build_uuri(
        &args.target_authority,
        target_uentity,
        target_uversion,
        target_resource,
    )?;

    let service_response_listener: Arc<dyn UListener> = Arc::new(ServiceResponseListener);
    client
        .register_listener(&sink, Some(&source), service_response_listener)
        .await?;

    let mut i = 0;
    loop {
        tokio::time::sleep(Duration::from_millis(1000)).await;

        let hello_request = HelloRequest {
            name: format!("ue_client@i={}", i).to_string(),
            ..Default::default()
        };
        i += 1;

        let request_msg = UMessageBuilder::request(sink.clone(), source.clone(), REQUEST_TTL)
            .build_with_protobuf_payload(&hello_request)
            .unwrap();
        debug!("Invoking URI {} with response URI {}", &sink, &source);
        info!("Sending Request message:\n{:?}", &request_msg);

        client.send(request_msg).await?;
    }
}

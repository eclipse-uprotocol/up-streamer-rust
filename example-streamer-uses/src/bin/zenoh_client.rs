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
use common::ServiceResponseListener;
use hello_world_protos::hello_world_service::HelloRequest;
use log::{debug, info};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use up_rust::{UListener, UMessageBuilder, UStatus, UTransport, UUri};
use up_transport_zenoh::UPTransportZenoh;
use zenoh::config::{Config, EndPoint};

const SERVICE_AUTHORITY: &str = "authority_A";
const SERVICE_UE_ID: u32 = 0x4321;
const SERVICE_UE_VERSION_MAJOR: u8 = 1;
const SERVICE_RESOURCE_ID: u16 = 0x0421;

const CLIENT_AUTHORITY: &str = "authority_B";
const CLIENT_UE_ID: u32 = 0x1236;
const CLIENT_UE_VERSION_MAJOR: u8 = 1;
const CLIENT_RESOURCE_ID: u16 = 0;

const REQUEST_TTL: u32 = 1000;

fn client_uuri() -> UUri {
    UUri::try_from_parts(
        CLIENT_AUTHORITY,
        CLIENT_UE_ID,
        CLIENT_UE_VERSION_MAJOR,
        CLIENT_RESOURCE_ID,
    )
    .unwrap()
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The endpoint for Zenoh client to connect to
    #[arg(short, long, default_value = "tcp/0.0.0.0:7445")]
    endpoint: String,
}

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    let args = Args::parse();

    info!("Started zenoh_client");

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

    let client_uri: String = (&client_uuri()).into();
    let client: Arc<dyn UTransport> = Arc::new(
        UPTransportZenoh::new(zenoh_config, client_uri)
            .await
            .unwrap(),
    );

    let source = UUri::try_from_parts(
        CLIENT_AUTHORITY,
        CLIENT_UE_ID,
        CLIENT_UE_VERSION_MAJOR,
        CLIENT_RESOURCE_ID,
    )
    .unwrap();
    let sink = UUri::try_from_parts(
        SERVICE_AUTHORITY,
        SERVICE_UE_ID,
        SERVICE_UE_VERSION_MAJOR,
        SERVICE_RESOURCE_ID,
    )
    .unwrap();

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

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
use hello_world_protos::hello_world_service::{HelloRequest, HelloResponse};
use log::trace;
use protobuf::Message;
use std::fs::canonicalize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use up_rust::{UListener, UMessage, UMessageBuilder, UStatus, UTransport, UUri};
use up_transport_vsomeip::UPTransportVsomeip;

const SERVICE_AUTHORITY: &str = "linux";
const SERVICE_UE_ID: u16 = 0x1236;
const SERVICE_UE_VERSION_MAJOR: u8 = 1;
const SERVICE_RESOURCE_ID: u16 = 0x0896;

const CLIENT_AUTHORITY: &str = "me_authority";
const CLIENT_UE_ID: u16 = 0x5678;
const CLIENT_UE_VERSION_MAJOR: u8 = 1;
const CLIENT_RESOURCE_ID: u16 = 0;

const REQUEST_TTL: u32 = 1000;

const REMOTE_AUTHORITY: &str = "linux";

struct ServiceResponseListener;

#[async_trait]
impl UListener for ServiceResponseListener {
    async fn on_receive(&self, msg: UMessage) {
        println!("ServiceResponseListener: Received a message: {msg:?}");

        let Some(payload_bytes) = msg.payload else {
            panic!("No payload bytes");
        };

        let Ok(hello_response) = HelloResponse::parse_from_bytes(&payload_bytes) else {
            panic!("Unable to parse into HelloResponse");
        };

        println!("Here we received response: {hello_response:?}");
    }

    async fn on_error(&self, err: UStatus) {
        println!("ServiceResponseListener: Encountered an error: {err:?}");
    }
}

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    println!("mE_client");

    let crate_dir = env!("CARGO_MANIFEST_DIR");
    // TODO: Make configurable to pass the path to the vsomeip config as a command line argument
    let vsomeip_config = PathBuf::from(crate_dir).join("vsomeip-configs/mE_client.json");
    let vsomeip_config = canonicalize(vsomeip_config).ok();
    trace!("vsomeip_config: {vsomeip_config:?}");

    // There will be a single vsomeip_transport, as there is a connection into device and a streamer
    // TODO: Add error handling if we fail to create a UPTransportVsomeip
    let client: Arc<dyn UTransport> = Arc::new(
        UPTransportVsomeip::new_with_config(
            &CLIENT_AUTHORITY.to_string(),
            &REMOTE_AUTHORITY.to_string(),
            CLIENT_UE_ID,
            &vsomeip_config.unwrap(),
            None,
        )
        .unwrap(),
    );

    let source = UUri {
        authority_name: CLIENT_AUTHORITY.to_string(),
        ue_id: CLIENT_UE_ID as u32,
        ue_version_major: CLIENT_UE_VERSION_MAJOR as u32,
        resource_id: CLIENT_RESOURCE_ID as u32,
        ..Default::default()
    };
    let sink = UUri {
        authority_name: SERVICE_AUTHORITY.to_string(),
        ue_id: SERVICE_UE_ID as u32,
        ue_version_major: SERVICE_UE_VERSION_MAJOR as u32,
        resource_id: SERVICE_RESOURCE_ID as u32,
        ..Default::default()
    };

    let service_response_listener: Arc<dyn UListener> = Arc::new(ServiceResponseListener);
    client
        .register_listener(&sink, Some(&source), service_response_listener)
        .await?;

    let mut i = 0;
    loop {
        tokio::time::sleep(Duration::from_millis(1000)).await;

        let hello_request = HelloRequest {
            name: format!("me_client@i={}", i).to_string(),
            ..Default::default()
        };
        i += 1;

        let request_msg = UMessageBuilder::request(sink.clone(), source.clone(), REQUEST_TTL)
            .build_with_protobuf_payload(&hello_request)
            .unwrap();
        println!("Sending Request message:\n{request_msg:?}");

        client.send(request_msg).await?;
    }
}

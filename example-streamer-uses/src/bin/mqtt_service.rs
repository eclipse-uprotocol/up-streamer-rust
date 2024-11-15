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

use common::ServiceRequestResponder;
use log::info;
use std::sync::Arc;
use std::thread;
use up_rust::{UListener, UStatus, UTransport, UUri, UUID};
use up_transport_mqtt5::{MqttConfig, UPClientMqtt, UPClientMqttType};

const SERVICE_AUTHORITY: &str = "authority_A";
const SERVICE_UE_ID: u32 = 0x4321;
const SERVICE_UE_VERSION_MAJOR: u8 = 1;
const SERVICE_RESOURCE_ID: u16 = 0x0421;

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    info!("Started mqtt_service.");

    // We set the source filter to "any" so that we process messages from all device that send some.
    let source_filter = UUri::any();
    // The sink filter gets specified so that we only process messages directed at this entity.
    let sink_filter = UUri::try_from_parts(
        SERVICE_AUTHORITY,
        SERVICE_UE_ID,
        SERVICE_UE_VERSION_MAJOR,
        SERVICE_RESOURCE_ID,
    )
    .unwrap();

    let ssl_options = None;

    let mqtt_config = MqttConfig {
        mqtt_protocol: up_transport_mqtt5::MqttProtocol::Mqtt,
        mqtt_port: 1883,
        mqtt_hostname: "localhost".to_string(),
        max_buffered_messages: 100,
        max_subscriptions: 100,
        session_expiry_interval: 3600,
        ssl_options,
        username: "user".to_string(),
    };

    let service: Arc<dyn UTransport> = Arc::new(
        UPClientMqtt::new(
            mqtt_config,
            UUID::build(),
            SERVICE_AUTHORITY.to_string(),
            UPClientMqttType::Device, // Todo: make sure that UPClientMqttType::Cloud also works
        )
        .await
        .expect("Could not create mqtt transport."),
    );

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

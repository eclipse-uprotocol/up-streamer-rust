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

use crate::config::{Config, SubscriptionProviderMode};
use clap::Parser;
use std::fs::File;
use std::io::Read;
use std::sync::Arc;
use std::{env, thread};
use tracing::trace;
use up_rust::core::usubscription::USubscription;
use up_rust::{UCode, UStatus, UTransport, UUri};
use up_streamer::{Endpoint, UStreamer};
use up_transport_vsomeip::UPTransportVsomeip;
use up_transport_zenoh::{zenoh_config::Config as ZenohConfig, UPTransportZenoh};
use usubscription_static_file::USubscriptionStaticFile;

#[derive(Parser)]
#[command()]
struct StreamerArgs {
    #[arg(short, long, value_name = "FILE")]
    config: String,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), UStatus> {
    let _ = tracing_subscriber::fmt::try_init();

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
    .await
    .expect("Failed to create uStreamer");

    let streamer_uuri = UUri::try_from_parts(
        &config.streamer_uuri.authority,
        config.streamer_uuri.ue_id,
        config.streamer_uuri.ue_version_major,
        0,
    )
    .expect("Unable to form streamer_uuri");

    trace!("streamer_uuri: {streamer_uuri:#?}");

    let zenoh_config = ZenohConfig::from_file(config.zenoh_transport_config.config_file).unwrap();

    let zenoh_transport: Arc<dyn UTransport> = Arc::new(
        UPTransportZenoh::builder(config.streamer_uuri.authority.clone())
            .expect("Unable to create Zenoh transport builder")
            .with_config(zenoh_config)
            .build()
            .await
            .expect("Unable to initialize Zenoh UTransport"),
    );

    // Because the streamer runs on the ecu side in this implementation, we call the zenoh endpoint the "host".
    let zenoh_endpoint = Endpoint::new(
        "host_endpoint",
        &config.streamer_uuri.authority,
        zenoh_transport.clone(),
    );

    let someip_config_file_abs_path = if config.someip_config.config_file.is_relative() {
        env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join(&config.someip_config.config_file)
    } else {
        config.someip_config.config_file
    };
    trace!("someip_config_file_abs_path: {someip_config_file_abs_path:?}");
    if !someip_config_file_abs_path.exists() {
        panic!("The specified someip config_file doesn't exist: {someip_config_file_abs_path:?}");
    }

    let host_uuri = UUri::try_from_parts(
        &config.streamer_uuri.authority,
        config
            .someip_config
            .default_someip_application_id_for_someip_subscriptions as u32,
        1,
        0,
    )
    .expect("Unable to make host_uuri");

    // There will be at most one vsomeip_transport, as there is a connection into device and a streamer
    let someip_transport: Arc<dyn UTransport> = Arc::new(
        UPTransportVsomeip::new_with_config(
            host_uuri,
            &config.someip_config.authority,
            &someip_config_file_abs_path,
            None,
        )
        .expect("Unable to initialize vsomeip UTransport"),
    );

    // In this implementation, the mqtt entity runs in the cloud and has its own authority.
    let someip_endpoint = Endpoint::new(
        "someip_endpoint",
        &config.someip_config.authority,
        someip_transport.clone(),
    );

    // Here we tell the streamer to forward any zenoh messages to the someip endpoint
    streamer
        .add_route(zenoh_endpoint.clone(), someip_endpoint.clone())
        .await
        .expect("Could not add zenoh -> someip forwarding rule");

    // And here we set up the forwarding in the other direction.
    streamer
        .add_route(someip_endpoint.clone(), zenoh_endpoint.clone())
        .await
        .expect("Could not add someip -> zenoh forwarding rule");

    thread::park();

    Ok(())
}

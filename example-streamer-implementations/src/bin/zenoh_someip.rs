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
use std::env;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, trace};
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

fn resolve_someip_config_file_path(config_file: &Path) -> Result<PathBuf, UStatus> {
    if !config_file.is_relative() {
        return Ok(config_file.to_path_buf());
    }

    let executable_path = env::current_exe().map_err(|error| {
        UStatus::fail_with_code(
            UCode::INTERNAL,
            format!("Unable to determine current executable path: {error:?}"),
        )
    })?;
    let executable_dir = executable_path.parent().ok_or_else(|| {
        UStatus::fail_with_code(
            UCode::INTERNAL,
            format!("Current executable has no parent directory: {executable_path:?}"),
        )
    })?;

    Ok(executable_dir.join(config_file))
}

async fn wait_for_shutdown_signal() -> Result<(), UStatus> {
    tokio::signal::ctrl_c().await.map_err(|error| {
        UStatus::fail_with_code(
            UCode::INTERNAL,
            format!("Unable to wait for shutdown signal: {error:?}"),
        )
    })
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
    .await?;

    let streamer_uuri = UUri::try_from_parts(
        &config.streamer_uuri.authority,
        config.streamer_uuri.ue_id,
        config.streamer_uuri.ue_version_major,
        0,
    )
    .map_err(|error| {
        UStatus::fail_with_code(
            UCode::INVALID_ARGUMENT,
            format!("Unable to form streamer_uuri: {error:?}"),
        )
    })?;

    trace!("streamer_uuri: {streamer_uuri:#?}");

    let zenoh_config =
        ZenohConfig::from_file(config.zenoh_transport_config.config_file).map_err(|error| {
            UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                format!("Unable to load Zenoh config file: {error:?}"),
            )
        })?;

    let zenoh_transport: Arc<dyn UTransport> = Arc::new(
        UPTransportZenoh::builder(config.streamer_uuri.authority.clone())
            .map_err(|error| {
                UStatus::fail_with_code(
                    UCode::INTERNAL,
                    format!("Unable to create Zenoh transport builder: {error:?}"),
                )
            })?
            .with_config(zenoh_config)
            .build()
            .await
            .map_err(|error| {
                UStatus::fail_with_code(
                    UCode::INTERNAL,
                    format!("Unable to initialize Zenoh UTransport: {error:?}"),
                )
            })?,
    );

    // Because the streamer runs on the ecu side in this implementation, we call the zenoh endpoint the "host".
    let zenoh_endpoint = Endpoint::new(
        "host_endpoint",
        &config.streamer_uuri.authority,
        zenoh_transport.clone(),
    );

    let someip_config_file_abs_path =
        resolve_someip_config_file_path(&config.someip_config.config_file)?;
    trace!("someip_config_file_abs_path: {someip_config_file_abs_path:?}");
    if !someip_config_file_abs_path.exists() {
        return Err(UStatus::fail_with_code(
            UCode::INVALID_ARGUMENT,
            format!(
                "The specified someip config_file doesn't exist: {someip_config_file_abs_path:?}"
            ),
        ));
    }

    let host_uuri = UUri::try_from_parts(
        &config.streamer_uuri.authority,
        config
            .someip_config
            .default_someip_application_id_for_someip_subscriptions as u32,
        1,
        0,
    )
    .map_err(|error| {
        UStatus::fail_with_code(
            UCode::INVALID_ARGUMENT,
            format!("Unable to make host_uuri: {error:?}"),
        )
    })?;

    // There will be at most one vsomeip_transport, as there is a connection into device and a streamer
    let someip_transport: Arc<dyn UTransport> = Arc::new(
        UPTransportVsomeip::new_with_config(
            host_uuri,
            &config.someip_config.authority,
            &someip_config_file_abs_path,
            None,
        )
        .map_err(|error| {
            UStatus::fail_with_code(
                UCode::INTERNAL,
                format!("Unable to initialize vsomeip UTransport: {error:?}"),
            )
        })?,
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
        .await?;

    // And here we set up the forwarding in the other direction.
    streamer
        .add_route(someip_endpoint.clone(), zenoh_endpoint.clone())
        .await?;

    println!("READY streamer_initialized");
    info!("Streamer initialized; waiting for shutdown signal");
    wait_for_shutdown_signal().await?;
    info!("Shutdown signal received; exiting");

    Ok(())
}

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

use crate::config::{Config, HostTransport};
use clap::Parser;
use log::trace;
use std::fs::File;
use std::io::Read;
use std::sync::Arc;
use std::{env, thread};
use up_rust::{UCode, UStatus, UTransport};
use up_streamer::{Endpoint, UStreamer};
use up_transport_vsomeip::UPTransportVsomeip;
use up_transport_zenoh::UPClientZenoh;
use usubscription_static_file::USubscriptionStaticFile;
use zenoh::config::Config as ZenohConfig;

#[derive(Parser)]
#[command()]
struct StreamerArgs {
    #[arg(short, long, value_name = "FILE")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

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

    let subscription_path = config.usubscription_config.file_path;
    let usubscription = Arc::new(USubscriptionStaticFile::new(subscription_path));

    let mut streamer = match UStreamer::new(
        "up-linux-streamer",
        config.up_streamer_config.message_queue_size,
        usubscription,
    ) {
        Ok(streamer) => streamer,
        Err(error) => panic!("Failed to create uStreamer: {}", error),
    };

    let zenoh_config = match ZenohConfig::from_file(config.zenoh_transport_config.config_file) {
        Ok(config) => {
            trace!("Able to read zenoh config from file");
            config
        }
        Err(error) => {
            panic!("Unable to read zenoh config from file: {}", error);
        }
    };

    let host_transport: Arc<dyn UTransport> = Arc::new(match config.host_config.transport {
        HostTransport::Zenoh => {
            UPClientZenoh::new(zenoh_config, config.host_config.authority.clone())
                .await
                .expect("Unable to initialize Zenoh UTransport")
        } // other host transports can be added here as they become available
    });

    let host_endpoint = Endpoint::new(
        "host_endpoint",
        &config.host_config.authority,
        host_transport.clone(),
    );

    if config.someip_config.enabled {
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
            panic!(
                "The specified someip config_file doesn't exist: {someip_config_file_abs_path:?}"
            );
        }

        // There will be at most one vsomeip_transport, as there is a connection into device and a streamer
        let someip_transport: Arc<dyn UTransport> = Arc::new(
            UPTransportVsomeip::new_with_config(
                &config.host_config.authority,
                &config.someip_config.authority,
                config
                    .someip_config
                    .default_someip_application_id_for_someip_subscriptions,
                &someip_config_file_abs_path,
                None,
            )
            .expect("Unable to initialize vsomeip UTransport"),
        );

        let mechatronics_endpoint = Endpoint::new(
            "mechatronics_endpoint",
            &config.someip_config.authority,
            someip_transport.clone(),
        );
        let forwarding_res = streamer
            .add_forwarding_rule(mechatronics_endpoint.clone(), host_endpoint.clone())
            .await;

        if let Err(err) = forwarding_res {
            panic!("Unable to add forwarding result: {err:?}");
        }

        let forwarding_res = streamer
            .add_forwarding_rule(host_endpoint.clone(), mechatronics_endpoint.clone())
            .await;

        if let Err(err) = forwarding_res {
            panic!("Unable to add forwarding result: {err:?}");
        }
    }

    thread::park();

    Ok(())
}

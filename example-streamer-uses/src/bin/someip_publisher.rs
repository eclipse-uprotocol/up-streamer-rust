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

use chrono::Local;
use chrono::Timelike;
use clap::Parser;
use common::cli;
use hello_world_protos::hello_world_topics::Timer;
use hello_world_protos::timeofday::TimeOfDay;
use log::{info, trace, warn};
use std::sync::Arc;
use std::time::Duration;
use up_rust::{UMessageBuilder, UStatus, UTransport};
use up_transport_vsomeip::UPTransportVsomeip;

const DEFAULT_UAUTHORITY: &str = "authority-a";
const DEFAULT_UENTITY: &str = "0x5BA0";
const DEFAULT_UVERSION: &str = "0x1";
const DEFAULT_RESOURCE: &str = "0x8001";
const DEFAULT_REMOTE_AUTHORITY: &str = "authority-b";
const DEFAULT_VSOMEIP_CONFIG: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/vsomeip-configs/someip_publisher.json"
);
const DEFAULT_UENTITY_NUM: u32 = 0x5BA0;

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// Authority for the local publisher identity and publish source URI
    #[arg(long, default_value = DEFAULT_UAUTHORITY)]
    uauthority: String,
    /// UEntity ID for local publisher identity (decimal or 0x-prefixed hex)
    #[arg(long, default_value = DEFAULT_UENTITY)]
    uentity: String,
    /// UEntity major version for local publisher identity (decimal or 0x-prefixed hex)
    #[arg(long, default_value = DEFAULT_UVERSION)]
    uversion: String,
    /// Resource ID for publish source URI (decimal or 0x-prefixed hex)
    #[arg(long, default_value = DEFAULT_RESOURCE)]
    resource: String,
    /// Remote authority used by the SOME/IP transport bridge
    #[arg(long, default_value = DEFAULT_REMOTE_AUTHORITY)]
    remote_authority: String,
    /// Path to the vsomeip JSON configuration file
    #[arg(long, default_value = DEFAULT_VSOMEIP_CONFIG)]
    vsomeip_config: String,
}

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    let args = Args::parse();

    info!("Started someip_publisher");

    let uentity = cli::parse_u32_status("--uentity", &args.uentity)?;
    let uversion = cli::parse_u8_status("--uversion", &args.uversion)?;
    let resource = cli::parse_u16_status("--resource", &args.resource)?;

    let vsomeip_config = cli::canonicalize_cli_path("--vsomeip-config", &args.vsomeip_config)?;
    trace!("vsomeip_config: {vsomeip_config:?}");

    if uentity != DEFAULT_UENTITY_NUM {
        warn!(
            "--uentity override ({uentity:#X}) may conflict with application/service IDs in '{}' ; update the SOME/IP config accordingly",
            vsomeip_config.display()
        );
    }

    let publisher_uuri = cli::build_uuri(&args.uauthority, uentity, uversion, 0)?;

    // There will be a single vsomeip_transport, as there is a connection into device and a streamer
    let publisher: Arc<dyn UTransport> = Arc::new(
        UPTransportVsomeip::new_with_config(
            publisher_uuri,
            &args.remote_authority,
            &vsomeip_config,
            None,
        )
        .unwrap(),
    );

    let source = cli::build_uuri(&args.uauthority, uentity, uversion, resource)?;

    loop {
        tokio::time::sleep(Duration::from_millis(1000)).await;

        let now = Local::now();

        let time_of_day = TimeOfDay {
            hours: now.hour() as i32,
            minutes: now.minute() as i32,
            seconds: now.second() as i32,
            nanos: now.nanosecond() as i32,
            ..Default::default()
        };

        let timer_message = Timer {
            time: Some(time_of_day).into(),
            ..Default::default()
        };

        let publish_msg = UMessageBuilder::publish(source.clone())
            .build_with_protobuf_payload(&timer_message)
            .unwrap();
        info!("Sending Publish message:\n{publish_msg:?}");

        publisher.send(publish_msg).await?;
    }
}

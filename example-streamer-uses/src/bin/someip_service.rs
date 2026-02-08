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
use log::{info, trace, warn};
use std::sync::Arc;
use std::thread;
use up_rust::{UListener, UStatus, UTransport, UUri};
use up_transport_vsomeip::UPTransportVsomeip;

const DEFAULT_UAUTHORITY: &str = "authority-a";
const DEFAULT_UENTITY: &str = "0x4321";
const DEFAULT_UVERSION: &str = "0x1";
const DEFAULT_RESOURCE: &str = "0x0421";
const DEFAULT_REMOTE_AUTHORITY: &str = "authority-b";
const DEFAULT_VSOMEIP_CONFIG: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/vsomeip-configs/someip_service.json"
);
const DEFAULT_UENTITY_NUM: u32 = 0x4321;

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

    info!("Started someip_service");

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

    let service_uuri = cli::build_uuri(&args.uauthority, uentity, uversion, 0)?;

    // There will be a single vsomeip_transport, as there is a connection into device and a streamer
    // TODO: Add error handling if we fail to create a UPTransportVsomeip
    let service: Arc<dyn UTransport> = Arc::new(
        UPTransportVsomeip::new_with_config(
            service_uuri,
            &args.remote_authority,
            &vsomeip_config,
            None,
        )
        .unwrap(),
    );

    let source_filter = UUri::any();
    let sink_filter = cli::build_uuri(&args.uauthority, uentity, uversion, resource)?;
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

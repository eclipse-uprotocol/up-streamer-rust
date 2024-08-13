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

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub(crate) __required__: bool,
    pub(crate) up_streamer_config: UpStreamerConfig,
    pub(crate) usubscription_config: USubscriptionConfig,
    pub(crate) host_config: HostConfig,
    pub(crate) someip_config: SomeipConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct UpStreamerConfig {
    pub(crate) message_queue_size: u16,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct USubscriptionConfig {
    pub(crate) file_path: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct HostConfig {
    pub(crate) transport: HostTransport,
    pub(crate) authority: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SomeipConfig {
    pub(crate) authority: String,
    pub(crate) config_file: PathBuf,
    pub(crate) default_someip_application_id_for_someip_subscriptions: u16,
    pub(crate) enabled: bool,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub enum HostTransport {
    Zenoh,
}

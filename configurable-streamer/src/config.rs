/********************************************************************************
 * Copyright (c) 2025 Contributors to the Eclipse Foundation
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

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub(crate) up_streamer_config: UpStreamerConfig,
    pub(crate) streamer_uuri: StreamerUuri,
    pub(crate) usubscription_config: USubscriptionConfig,
    pub(crate) transports: Transports,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct UpStreamerConfig {
    pub(crate) message_queue_size: u16,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct StreamerUuri {
    pub(crate) authority: String,
    pub(crate) ue_id: u32,
    pub(crate) ue_version_major: u8,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct USubscriptionConfig {
    #[serde(default)]
    pub(crate) mode: SubscriptionProviderMode,
    pub(crate) file_path: String,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionProviderMode {
    #[default]
    StaticFile,
    LiveUsubscription,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Transports {
    pub(crate) zenoh: ZenohTransport,
    pub(crate) mqtt: MqttTransport,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct ZenohTransport {
    pub(crate) config_file: String,
    pub(crate) endpoints: Vec<EndpointConfig>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct MqttTransport {
    pub(crate) config_file: String,
    pub(crate) endpoints: Vec<EndpointConfig>,
    #[serde(skip)]
    pub(crate) mqtt_details: Option<MqttConfigDetails>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct EndpointConfig {
    pub(crate) authority: String,
    pub(crate) endpoint: String,
    pub(crate) forwarding: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct MqttConfigDetails {
    pub(crate) hostname: String,
    pub(crate) port: u16,
    pub(crate) max_buffered_messages: i32,
    pub(crate) max_subscriptions: i32,
    pub(crate) session_expiry_interval: i32,
    pub(crate) username: String,
}

impl MqttTransport {
    pub fn load_mqtt_details(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let config_contents = std::fs::read_to_string(&self.config_file)?;
        self.mqtt_details = Some(json5::from_str(&config_contents)?);
        Ok(())
    }
}

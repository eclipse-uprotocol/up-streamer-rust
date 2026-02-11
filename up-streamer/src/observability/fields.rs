/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
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

//! Canonical structured field keys and value-format helpers.

use up_rust::{UAttributes, UMessage, UUri};

pub const EVENT: &str = "event";
pub const COMPONENT: &str = "component";
pub const WORKER_ID: &str = "worker_id";
pub const WORKER_THREAD: &str = "worker_thread";
pub const ROUTE_LABEL: &str = "route_label";

pub const MSG_ID: &str = "msg_id";
pub const MSG_TYPE: &str = "msg_type";
pub const SRC: &str = "src";
pub const SINK: &str = "sink";

pub const REF_COUNT: &str = "ref_count";
pub const SKIPPED: &str = "skipped";
pub const REASON: &str = "reason";
pub const ERR: &str = "err";
pub const IN_AUTHORITY: &str = "in_authority";
pub const OUT_AUTHORITY: &str = "out_authority";
pub const SOURCE_FILTER: &str = "source_filter";
pub const SINK_FILTER: &str = "sink_filter";

pub const NONE: &str = "none";
pub const REASON_BROADCAST_CLOSED: &str = "broadcast_closed";
pub const REASON_INVALID_THREAD_NAME: &str = "invalid_thread_name";
pub const DEFAULT_WORKER_THREAD: &str = "unknown-thread";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerContext {
    pub worker_id: String,
    pub worker_thread: String,
}

impl WorkerContext {
    pub fn new(worker_id: impl Into<String>, worker_thread: Option<&str>) -> Self {
        Self {
            worker_id: worker_id.into(),
            worker_thread: thread_name_or_default(worker_thread),
        }
    }

    pub fn with_current_thread(worker_id: impl Into<String>) -> Self {
        Self {
            worker_id: worker_id.into(),
            worker_thread: current_thread_name_or_default(),
        }
    }
}

pub fn thread_name_or_default(thread_name: Option<&str>) -> String {
    thread_name.unwrap_or(DEFAULT_WORKER_THREAD).to_string()
}

pub fn current_thread_name_or_default() -> String {
    thread_name_or_default(std::thread::current().name())
}

pub fn format_message_id(message: &UMessage) -> String {
    message
        .attributes
        .as_ref()
        .and_then(|attributes| attributes.id.as_ref())
        .map(|id| id.to_hyphenated_string())
        .unwrap_or_else(|| NONE.to_string())
}

pub fn format_message_type(message: &UMessage) -> String {
    message
        .attributes
        .as_ref()
        .map(|attributes| format!("{:?}", attributes.type_.enum_value_or_default()))
        .unwrap_or_else(|| "UMESSAGE_TYPE_UNSPECIFIED".to_string())
}

pub fn format_source_uri(message: &UMessage) -> String {
    format_optional_uri(
        message
            .attributes
            .as_ref()
            .and_then(|attributes| attributes.source.as_ref()),
    )
}

pub fn format_sink_uri(message: &UMessage) -> String {
    format_optional_uri(
        message
            .attributes
            .as_ref()
            .and_then(|attributes| attributes.sink.as_ref()),
    )
}

pub fn format_uri(uri: &UUri) -> String {
    uri.to_uri(false).trim_start_matches("//").to_string()
}

fn format_optional_uri(uri: Option<&UUri>) -> String {
    uri.map(format_uri).unwrap_or_else(|| NONE.to_string())
}

pub fn format_attributes_message_id(attributes: Option<&UAttributes>) -> String {
    attributes
        .and_then(|attrs| attrs.id.as_ref())
        .map(|id| id.to_hyphenated_string())
        .unwrap_or_else(|| NONE.to_string())
}

pub fn format_attributes_message_type(attributes: Option<&UAttributes>) -> String {
    attributes
        .map(|attrs| format!("{:?}", attrs.type_.enum_value_or_default()))
        .unwrap_or_else(|| "UMESSAGE_TYPE_UNSPECIFIED".to_string())
}

pub fn format_attributes_source_uri(attributes: Option<&UAttributes>) -> String {
    format_optional_uri(attributes.and_then(|attrs| attrs.source.as_ref()))
}

pub fn format_attributes_sink_uri(attributes: Option<&UAttributes>) -> String {
    format_optional_uri(attributes.and_then(|attrs| attrs.sink.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::{
        format_message_id, format_sink_uri, format_source_uri, thread_name_or_default,
        DEFAULT_WORKER_THREAD, NONE,
    };
    use up_rust::{UAttributes, UMessage, UUri, UUID};

    #[test]
    fn format_message_id_returns_uuid_when_present() {
        let message_id = UUID::build();
        let message = UMessage {
            attributes: Some(UAttributes {
                id: Some(message_id.clone()).into(),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        };

        assert_eq!(
            format_message_id(&message),
            message_id.to_hyphenated_string()
        );
    }

    #[test]
    fn format_message_id_returns_none_when_absent() {
        let message = UMessage::default();

        assert_eq!(format_message_id(&message), NONE);
    }

    #[test]
    fn format_sink_uri_returns_uri_when_present() {
        let sink = UUri::try_from_parts("authority-b", 0x5678, 0x1, 0x1234)
            .expect("sink URI should build");
        let message = UMessage {
            attributes: Some(UAttributes {
                sink: Some(sink).into(),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        };

        assert_eq!(format_sink_uri(&message), "authority-b/5678/1/1234");
    }

    #[test]
    fn format_sink_uri_returns_none_when_absent() {
        let message = UMessage::default();

        assert_eq!(format_sink_uri(&message), NONE);
    }

    #[test]
    fn format_source_uri_is_stable_compact_path() {
        let source = UUri::try_from_parts("authority-a", 0x5ba0, 0x1, 0x8001)
            .expect("source URI should build");
        let message = UMessage {
            attributes: Some(UAttributes {
                source: Some(source).into(),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        };

        assert_eq!(format_source_uri(&message), "authority-a/5BA0/1/8001");
    }

    #[test]
    fn thread_name_or_default_falls_back_when_absent() {
        assert_eq!(thread_name_or_default(None), DEFAULT_WORKER_THREAD);
        assert_eq!(thread_name_or_default(Some("named-thread")), "named-thread");
    }
}

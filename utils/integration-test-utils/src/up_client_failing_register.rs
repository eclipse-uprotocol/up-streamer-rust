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
use std::sync::Arc;
use tracing::debug;
use up_rust::{UCode, UListener, UMessage, UStatus, UTransport, UUri};

pub struct UPClientFailingRegister {
    name: Arc<String>,
}

impl UPClientFailingRegister {
    pub async fn new(name: &str) -> Self {
        let name = Arc::new(name.to_string());

        Self { name }
    }
}

#[async_trait]
impl UTransport for UPClientFailingRegister {
    async fn send(&self, message: UMessage) -> Result<(), UStatus> {
        debug!("sending: {message:?}");
        Ok(())
    }

    async fn receive(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
    ) -> Result<UMessage, UStatus> {
        unimplemented!()
    }

    async fn register_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        _listener: Arc<dyn UListener>,
    ) -> Result<(), UStatus> {
        debug!(
            "{}: registering listener for: source: {:?} sink: {:?}",
            self.name, source_filter, sink_filter
        );
        Err(UStatus::fail_with_code(
            UCode::INVALID_ARGUMENT,
            "Failing to register listener",
        ))
    }

    async fn unregister_listener(
        &self,
        source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UListener>,
    ) -> Result<(), UStatus> {
        debug!(
            "{} unregistering listener for source_filter: {source_filter:?}",
            &self.name
        );
        Ok(())
    }
}

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

use async_std::sync::Mutex;
use async_trait::async_trait;
use log::debug;
use std::sync::Arc;
use up_rust::{UListener, UMessage, UStatus};

#[derive(Clone)]
pub struct LocalClientListener {
    message_store: Arc<Mutex<Vec<UMessage>>>,
}

impl LocalClientListener {
    pub fn new() -> Self {
        Self {
            message_store: Arc::new(Mutex::new(Vec::with_capacity(10000))),
        }
    }

    pub fn retrieve_message_store(&self) -> Arc<Mutex<Vec<UMessage>>> {
        self.message_store.clone()
    }
}

#[async_trait]
impl UListener for LocalClientListener {
    async fn on_receive(&self, msg: UMessage) {
        self.message_store.lock().await.push(msg.clone());
        debug!("within local_client_listener! msg: {:?}", msg);
    }

    async fn on_error(&self, err: UStatus) {
        debug!("within local_client_listener! err: {:?}", err);
    }
}

impl Default for LocalClientListener {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct RemoteClientListener {
    message_store: Arc<Mutex<Vec<UMessage>>>,
}

impl RemoteClientListener {
    pub fn new() -> Self {
        Self {
            message_store: Arc::new(Mutex::new(Vec::with_capacity(10000))),
        }
    }

    pub fn retrieve_message_store(&self) -> Arc<Mutex<Vec<UMessage>>> {
        self.message_store.clone()
    }
}

#[async_trait]
impl UListener for RemoteClientListener {
    async fn on_receive(&self, msg: UMessage) {
        self.message_store.lock().await.push(msg.clone());
        debug!("within remote_client_listener! msg: {:?}", msg);
    }

    async fn on_error(&self, err: UStatus) {
        debug!("within remote_client_listener! err: {:?}", err);
    }
}

impl Default for RemoteClientListener {
    fn default() -> Self {
        Self::new()
    }
}

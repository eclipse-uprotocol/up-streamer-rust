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

use async_std::sync::{Arc, Mutex};
use log::*;
use up_rust::{UAuthority, UTransport};

const ROUTE_TAG: &str = "Route:";
const ROUTEFN_NEW_TAG: &str = "new():";

#[derive(Clone)]
pub struct Route {
    pub(crate) authority: UAuthority,
    pub(crate) transport: Arc<Mutex<Box<dyn UTransport>>>,
}

impl Route {
    pub fn new(authority: UAuthority, transport: Arc<Mutex<Box<dyn UTransport>>>) -> Self {
        // Try to initiate logging.
        // Required in case of dynamic lib, otherwise no logs.
        // But cannot be done twice in case of static link.
        let _ = env_logger::try_init();
        debug!(
            "{}:{} Creating Route from: ({:?})",
            &ROUTE_TAG, &ROUTEFN_NEW_TAG, &authority,
        );
        Self {
            authority,
            transport,
        }
    }
}

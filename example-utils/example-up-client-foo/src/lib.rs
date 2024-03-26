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

use up_rust::{UMessage, UStatus};

mod up_client_foo;

pub type UTransportListener = Box<dyn Fn(Result<UMessage, UStatus>) + Send + Sync + 'static>;

pub use up_client_foo::{UPClientFoo, UTransportBuilderFoo};

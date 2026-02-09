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

mod up_client_foo;
pub use up_client_foo::UPClientFoo;

mod up_client_failing_register;
pub use up_client_failing_register::UPClientFailingRegister;

mod integration_test_utils;

pub use integration_test_utils::{
    check_messages_in_order, check_send_receive_message_discrepancy, reset_pause, run_client,
    signal_to_pause, signal_to_resume, wait_for_pause, ClientCommand, ClientConfiguration,
    ClientControl, ClientHistory, ClientMessages, Signal,
};
mod integration_test_listeners;
pub use integration_test_listeners::{LocalClientListener, RemoteClientListener};
mod integration_test_uuris;

pub use integration_test_uuris::{
    local_authority, local_client_uuri, remote_authority_a, remote_authority_b, remote_client_uuri,
};
mod integration_test_messages;
pub use integration_test_messages::{
    notification_from_local_client_for_remote_client,
    notification_from_remote_client_for_local_client, publish_from_local_client_for_remote_client,
    publish_from_remote_client_for_local_client, request_from_local_client_for_remote_client,
    request_from_remote_client_for_local_client, response_from_local_client_for_remote_client,
    response_from_remote_client_for_local_client,
};

/// Helper method for integration tests to initialise tracing.
/// `RUST_LOG` env is read. Defaults to `debug`.
pub fn init_logging() {
    let log_filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "debug".to_string());
    let _ = tracing_subscriber::fmt()
        .with_env_filter(log_filter)
        .without_time()
        .with_test_writer()
        .try_init();
}

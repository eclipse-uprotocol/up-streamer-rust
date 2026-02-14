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

use integration_test_utils::{local_authority, remote_authority_a, UPClientFailingRegister};
use std::sync::Arc;
use up_rust::{UCode, UStatus, UTransport};
use up_streamer::{Endpoint, UStreamer};
use usubscription_static_file::USubscriptionStaticFile;

#[tokio::test(flavor = "multi_thread")]
async fn usubscription_bad_data() {
    integration_test_utils::init_logging();

    let utransport_foo: Arc<dyn UTransport> =
        Arc::new(UPClientFailingRegister::new("upclient_foo").await);
    let utransport_bar: Arc<dyn UTransport> =
        Arc::new(UPClientFailingRegister::new("upclient_bar").await);

    // setting up streamer to bridge between "foo" and "bar"
    let subscription_path =
        "../utils/usubscription-static-file/static-configs/testdata.json".to_string();
    let usubscription = Arc::new(USubscriptionStaticFile::new(subscription_path));
    let mut ustreamer = match UStreamer::new("foo_bar_streamer", 3000, usubscription).await {
        Ok(streamer) => streamer,
        Err(error) => panic!("Failed to create uStreamer: {}", error),
    };

    // setting up endpoints between authorities and protocols
    let local_endpoint = Endpoint::new("local_endpoint", &local_authority(), utransport_foo);
    let remote_endpoint = Endpoint::new("remote_endpoint", &remote_authority_a(), utransport_bar);

    // adding local to remote routing
    let add_route_res = ustreamer
        .add_route(local_endpoint.clone(), remote_endpoint.clone())
        .await;
    assert_eq!(
        add_route_res,
        Err(UStatus::fail_with_code(
            UCode::INVALID_ARGUMENT,
            "Failed to register notification request/response listener"
        ))
    );
}

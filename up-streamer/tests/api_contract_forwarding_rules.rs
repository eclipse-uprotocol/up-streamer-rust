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
use up_rust::{UCode, UListener, UMessage, UStatus, UTransport, UUri};
use up_streamer::{Endpoint, UStreamer};
use usubscription_static_file::USubscriptionStaticFile;

struct NoopTransport;

#[async_trait]
impl UTransport for NoopTransport {
    async fn send(&self, _message: UMessage) -> Result<(), UStatus> {
        Ok(())
    }

    async fn receive(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
    ) -> Result<UMessage, UStatus> {
        Err(UStatus::fail_with_code(
            UCode::UNIMPLEMENTED,
            "not used in tests",
        ))
    }

    async fn register_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UListener>,
    ) -> Result<(), UStatus> {
        Ok(())
    }

    async fn unregister_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UListener>,
    ) -> Result<(), UStatus> {
        Ok(())
    }
}

fn make_streamer() -> UStreamer {
    let subscription_path =
        "../utils/usubscription-static-file/static-configs/testdata.json".to_string();
    let usubscription = Arc::new(USubscriptionStaticFile::new(subscription_path));
    UStreamer::new("api-contract-test", 32, usubscription)
        .expect("streamer creation should succeed")
}

#[tokio::test(flavor = "multi_thread")]
async fn add_delete_forwarding_rule_contract_duplicate_and_missing_rules() {
    integration_test_utils::init_logging();

    let local_transport: Arc<dyn UTransport> = Arc::new(NoopTransport);
    let remote_transport: Arc<dyn UTransport> = Arc::new(NoopTransport);

    let local_endpoint = Endpoint::new("local", "local-authority", local_transport);
    let remote_endpoint = Endpoint::new("remote", "remote-authority", remote_transport);

    let mut streamer = make_streamer();

    assert_eq!(
        streamer
            .add_forwarding_rule(local_endpoint.clone(), remote_endpoint.clone())
            .await,
        Ok(())
    );
    assert_eq!(
        streamer
            .add_forwarding_rule(local_endpoint.clone(), remote_endpoint.clone())
            .await,
        Err(UStatus::fail_with_code(
            UCode::ALREADY_EXISTS,
            "already exists"
        ))
    );

    assert_eq!(
        streamer
            .delete_forwarding_rule(local_endpoint.clone(), remote_endpoint.clone())
            .await,
        Ok(())
    );
    assert_eq!(
        streamer
            .delete_forwarding_rule(local_endpoint, remote_endpoint)
            .await,
        Err(UStatus::fail_with_code(UCode::NOT_FOUND, "not found"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn add_delete_forwarding_rule_contract_rejects_same_authority() {
    integration_test_utils::init_logging();

    let transport: Arc<dyn UTransport> = Arc::new(NoopTransport);
    let endpoint_a = Endpoint::new("a", "shared-authority", transport.clone());
    let endpoint_b = Endpoint::new("b", "shared-authority", transport);
    let mut streamer = make_streamer();

    let add_error = streamer
        .add_forwarding_rule(endpoint_a.clone(), endpoint_b.clone())
        .await
        .expect_err("same-authority add should fail");
    assert_eq!(
        add_error.code.enum_value_or_default(),
        UCode::INVALID_ARGUMENT
    );

    let delete_error = streamer
        .delete_forwarding_rule(endpoint_a, endpoint_b)
        .await
        .expect_err("same-authority delete should fail");
    assert_eq!(
        delete_error.code.enum_value_or_default(),
        UCode::INVALID_ARGUMENT
    );
}

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

#![allow(clippy::mutable_key_type)]

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashSet;
use std::fs::{self, canonicalize};
use std::path::PathBuf;
use std::str::FromStr;
use tracing::{debug, error, warn};
use up_rust::core::usubscription::{
    FetchSubscribersRequest, FetchSubscribersResponse, FetchSubscriptionsRequest,
    FetchSubscriptionsResponse, NotificationsRequest, ResetRequest, ResetResponse, SubscriberInfo,
    Subscription, SubscriptionRequest, SubscriptionResponse, USubscription, UnsubscribeRequest,
};
use up_rust::{UCode, UStatus, UUri};

pub struct USubscriptionStaticFile {
    static_file: String,
}

impl USubscriptionStaticFile {
    pub fn new(static_file: String) -> Self {
        USubscriptionStaticFile { static_file }
    }
}

#[async_trait]
impl USubscription for USubscriptionStaticFile {
    async fn subscribe(
        &self,
        _subscription_request: SubscriptionRequest,
    ) -> Result<SubscriptionResponse, UStatus> {
        todo!()
    }

    async fn fetch_subscriptions(
        &self,
        fetch_subscriptions_request: FetchSubscriptionsRequest,
    ) -> Result<FetchSubscriptionsResponse, UStatus> {
        // Reads in a file and builds it into a subscription_cache data type
        // This is a static file, so we will just return the same set of subscribers
        // for all URIs
        debug!(
            "fetch_subscriptions for topic: {}",
            fetch_subscriptions_request.subscriber()
        );

        let subscription_json_file = PathBuf::from(self.static_file.clone());

        let mut subscriptions_vec = Vec::new();

        debug!("subscription_json_file: {subscription_json_file:?}");
        let canonicalized_result = canonicalize(subscription_json_file);
        debug!("canonicalize: {canonicalized_result:?}",);

        let subscription_json_file = match canonicalized_result {
            Ok(path) => path,
            Err(e) => {
                return Err(UStatus::fail_with_code(
                    UCode::INVALID_ARGUMENT,
                    format!("Static subscription file not found: {e:?}"),
                ))
            }
        };

        let data = fs::read_to_string(subscription_json_file).map_err(|e| {
            UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                format!("Unable to read file: {e:?}"),
            )
        })?;

        let res: Value = serde_json::from_str(&data).map_err(|e| {
            UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                format!("Unable to parse JSON: {e:?}"),
            )
        })?;

        if let Some(obj) = res.as_object() {
            for (key, value) in obj {
                debug!("key: {key}, value: {value}");
                let mut subscriber_set: HashSet<UUri> = HashSet::new();

                if let Some(array) = value.as_array() {
                    for subscriber in array {
                        debug!("Reading subscriber as URI: {subscriber}");

                        if let Some(subscriber_str) = subscriber.as_str() {
                            match UUri::from_str(subscriber_str) {
                                Ok(uri) => {
                                    debug!("All good for subscriber: {uri}");
                                    subscriber_set.insert(uri);
                                }
                                Err(error) => {
                                    error!("Error with Deserializing Subscriber '{subscriber_str}': {error}");
                                }
                            }
                        } else {
                            warn!("Unable to parse subscriber '{subscriber}");
                        }
                    }
                }

                debug!("key: {key}");
                let topic = match UUri::from_str(&key.to_string()) {
                    Ok(mut uri) => {
                        debug!("All good for key '{key}'");
                        warn!("Setting fixed resourceid 0x8001 for uri '{uri}'");
                        uri.resource_id = 0x8001;
                        uri
                    }
                    Err(error) => {
                        error!("Error with Deserializing Key: {error}");
                        continue;
                    }
                };

                for subscriber in subscriber_set {
                    let subscriber_info_tmp = SubscriberInfo {
                        uri: Some(subscriber).into(),
                        ..Default::default()
                    };
                    let subscription_tmp = Subscription {
                        topic: Some(topic.clone()).into(),
                        subscriber: Some(subscriber_info_tmp).into(),
                        ..Default::default()
                    };
                    subscriptions_vec.push(subscription_tmp);
                }
            }
        }
        debug!("Finished reading Subscriptions\n{subscriptions_vec:#?}");
        let fetch_response = FetchSubscriptionsResponse {
            subscriptions: subscriptions_vec,
            ..Default::default()
        };

        Ok(fetch_response)
    }

    async fn unsubscribe(&self, _unsubscribe_request: UnsubscribeRequest) -> Result<(), UStatus> {
        todo!()
    }

    async fn register_for_notifications(
        &self,
        _notifications_register_request: NotificationsRequest,
    ) -> Result<(), UStatus> {
        Ok(())
    }

    async fn unregister_for_notifications(
        &self,
        _notifications_unregister_request: NotificationsRequest,
    ) -> Result<(), UStatus> {
        todo!()
    }

    async fn fetch_subscribers(
        &self,
        _fetch_subscribers_request: FetchSubscribersRequest,
    ) -> Result<FetchSubscribersResponse, UStatus> {
        todo!();
    }

    async fn reset(&self, _reset_request: ResetRequest) -> Result<ResetResponse, UStatus> {
        Ok(ResetResponse::default())
    }
}

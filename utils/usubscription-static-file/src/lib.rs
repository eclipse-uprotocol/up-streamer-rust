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
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, canonicalize};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use tracing::{debug, error, warn};
use up_rust::core::usubscription::{
    FetchSubscribersRequest, FetchSubscribersResponse, FetchSubscriptionsRequest,
    FetchSubscriptionsResponse, NotificationsRequest, ResetRequest, ResetResponse, SubscriberInfo,
    Subscription, SubscriptionRequest, SubscriptionResponse, USubscription, UnsubscribeRequest,
};
use up_rust::{UCode, UStatus, UUri};

const STATIC_RESOURCE_ID: u32 = 0x8001;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StaticFileReloadStrategy {
    #[default]
    AlwaysReload,
    CacheOnFirstRead,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct UriProjectionKey {
    authority_name: String,
    ue_id: u32,
    ue_version_major: u8,
    resource_id: u16,
}

impl From<UUri> for UriProjectionKey {
    fn from(uri: UUri) -> Self {
        let ue_version_major = uri.uentity_major_version();
        let resource_id = uri.resource_id();

        Self {
            authority_name: uri.authority_name,
            ue_id: uri.ue_id,
            ue_version_major,
            resource_id,
        }
    }
}

impl From<&UUri> for UriProjectionKey {
    fn from(uri: &UUri) -> Self {
        Self {
            authority_name: uri.authority_name.clone(),
            ue_id: uri.ue_id,
            ue_version_major: uri.uentity_major_version(),
            resource_id: uri.resource_id(),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct SubscriptionIdentityKey {
    topic: UriProjectionKey,
    subscriber: UriProjectionKey,
}

pub struct USubscriptionStaticFile {
    static_file: String,
    reload_strategy: StaticFileReloadStrategy,
    cached_subscriptions: RwLock<Option<Arc<Vec<Subscription>>>>,
}

impl USubscriptionStaticFile {
    pub fn new(static_file: String) -> Self {
        Self::with_reload_strategy(static_file, StaticFileReloadStrategy::AlwaysReload)
    }

    pub fn with_reload_strategy(
        static_file: String,
        reload_strategy: StaticFileReloadStrategy,
    ) -> Self {
        Self {
            static_file,
            reload_strategy,
            cached_subscriptions: RwLock::new(None),
        }
    }

    pub fn clear_cached_subscriptions(&self) -> Result<(), UStatus> {
        let mut cache_guard = self.cached_subscriptions.write().map_err(|_| {
            UStatus::fail_with_code(
                UCode::INTERNAL,
                "Static subscription cache write lock is poisoned",
            )
        })?;
        *cache_guard = None;
        Ok(())
    }

    fn unsupported_operation_status(operation: &str) -> UStatus {
        UStatus::fail_with_code(
            UCode::UNIMPLEMENTED,
            format!("{operation} is not supported by USubscriptionStaticFile (read-only backend)"),
        )
    }

    fn canonicalized_static_file_path(&self) -> Result<PathBuf, UStatus> {
        let subscription_json_file = PathBuf::from(self.static_file.clone());
        debug!("subscription_json_file: {subscription_json_file:?}");

        let canonicalized_result = canonicalize(subscription_json_file);
        debug!("canonicalize: {canonicalized_result:?}");

        canonicalized_result.map_err(|error| {
            UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                format!("Static subscription file not found: {error:?}"),
            )
        })
    }

    fn read_static_config_json(&self) -> Result<Value, UStatus> {
        let subscription_json_file = self.canonicalized_static_file_path()?;
        let data = fs::read_to_string(subscription_json_file).map_err(|error| {
            UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                format!("Unable to read file: {error:?}"),
            )
        })?;

        serde_json::from_str(&data).map_err(|error| {
            UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                format!("Unable to parse JSON: {error:?}"),
            )
        })
    }

    fn parse_static_subscriptions(&self) -> Result<Vec<Subscription>, UStatus> {
        let value = self.read_static_config_json()?;
        let Some(entries) = value.as_object() else {
            return Err(UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                "Static subscription file must be a JSON object mapping topic URI keys to arrays of subscriber URI strings",
            ));
        };

        let mut subscriptions_by_key: HashMap<SubscriptionIdentityKey, Subscription> =
            HashMap::new();

        for (topic_key, subscriber_values) in entries {
            let mut topic = match UUri::from_str(topic_key) {
                Ok(uri) => uri,
                Err(error) => {
                    error!("Error deserializing topic '{topic_key}': {error}");
                    continue;
                }
            };

            if topic.resource_id != STATIC_RESOURCE_ID {
                warn!("Setting fixed resource_id {STATIC_RESOURCE_ID:#06X} for topic '{topic}'");
                topic.resource_id = STATIC_RESOURCE_ID;
            }

            let Some(subscribers) = subscriber_values.as_array() else {
                warn!("Ignoring non-array subscriber list for topic '{topic_key}'");
                continue;
            };

            for subscriber_value in subscribers {
                let Some(subscriber_str) = subscriber_value.as_str() else {
                    warn!("Unable to parse subscriber '{subscriber_value}'");
                    continue;
                };

                let subscriber_uri = match UUri::from_str(subscriber_str) {
                    Ok(uri) => uri,
                    Err(error) => {
                        error!("Error deserializing subscriber '{subscriber_str}': {error}");
                        continue;
                    }
                };

                let subscription_identity = SubscriptionIdentityKey {
                    topic: UriProjectionKey::from(&topic),
                    subscriber: UriProjectionKey::from(&subscriber_uri),
                };

                subscriptions_by_key
                    .entry(subscription_identity)
                    .or_insert_with(|| Subscription {
                        topic: Some(topic.clone()).into(),
                        subscriber: Some(SubscriberInfo {
                            uri: Some(subscriber_uri).into(),
                            ..Default::default()
                        })
                        .into(),
                        ..Default::default()
                    });
            }
        }

        Ok(subscriptions_by_key.into_values().collect())
    }

    fn load_subscriptions(&self) -> Result<Arc<Vec<Subscription>>, UStatus> {
        match self.reload_strategy {
            StaticFileReloadStrategy::AlwaysReload => {
                Ok(Arc::new(self.parse_static_subscriptions()?))
            }
            StaticFileReloadStrategy::CacheOnFirstRead => {
                {
                    let cache_guard = self.cached_subscriptions.read().map_err(|_| {
                        UStatus::fail_with_code(
                            UCode::INTERNAL,
                            "Static subscription cache read lock is poisoned",
                        )
                    })?;

                    if let Some(cached_subscriptions) = cache_guard.as_ref() {
                        return Ok(cached_subscriptions.clone());
                    }
                }

                let parsed_subscriptions = Arc::new(self.parse_static_subscriptions()?);
                let mut cache_guard = self.cached_subscriptions.write().map_err(|_| {
                    UStatus::fail_with_code(
                        UCode::INTERNAL,
                        "Static subscription cache write lock is poisoned",
                    )
                })?;

                if let Some(cached_subscriptions) = cache_guard.as_ref() {
                    return Ok(cached_subscriptions.clone());
                }

                *cache_guard = Some(parsed_subscriptions.clone());
                Ok(parsed_subscriptions)
            }
        }
    }
}

#[async_trait]
impl USubscription for USubscriptionStaticFile {
    async fn subscribe(
        &self,
        _subscription_request: SubscriptionRequest,
    ) -> Result<SubscriptionResponse, UStatus> {
        Err(Self::unsupported_operation_status("subscribe"))
    }

    async fn fetch_subscriptions(
        &self,
        fetch_subscriptions_request: FetchSubscriptionsRequest,
    ) -> Result<FetchSubscriptionsResponse, UStatus> {
        debug!("fetch_subscriptions request: {fetch_subscriptions_request:?}");

        let subscriptions = self.load_subscriptions()?;
        debug!("Finished reading subscriptions\n{subscriptions:#?}");

        Ok(FetchSubscriptionsResponse {
            subscriptions: subscriptions.as_ref().clone(),
            ..Default::default()
        })
    }

    async fn unsubscribe(&self, _unsubscribe_request: UnsubscribeRequest) -> Result<(), UStatus> {
        Err(Self::unsupported_operation_status("unsubscribe"))
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
        Ok(())
    }

    async fn fetch_subscribers(
        &self,
        fetch_subscribers_request: FetchSubscribersRequest,
    ) -> Result<FetchSubscribersResponse, UStatus> {
        let requested_topic = fetch_subscribers_request.topic.as_ref().ok_or_else(|| {
            UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                "fetch_subscribers requires a topic",
            )
        })?;

        let mut canonical_topic = requested_topic.clone();
        canonical_topic.resource_id = STATIC_RESOURCE_ID;
        let requested_topic_identity = UriProjectionKey::from(canonical_topic);

        let subscriptions = self.load_subscriptions()?;
        let mut subscribers_by_key: HashMap<UriProjectionKey, SubscriberInfo> = HashMap::new();

        for subscription in subscriptions.iter().cloned() {
            let Some(topic) = subscription.topic.into_option() else {
                continue;
            };
            if UriProjectionKey::from(topic) != requested_topic_identity {
                continue;
            }

            let Some(subscriber) = subscription.subscriber.into_option() else {
                continue;
            };
            let Some(subscriber_uri) = subscriber.uri.as_ref() else {
                continue;
            };

            subscribers_by_key
                .entry(UriProjectionKey::from(subscriber_uri))
                .or_insert(subscriber);
        }

        Ok(FetchSubscribersResponse {
            subscribers: subscribers_by_key.into_values().collect(),
            ..Default::default()
        })
    }

    async fn reset(&self, _reset_request: ResetRequest) -> Result<ResetResponse, UStatus> {
        Ok(ResetResponse::default())
    }
}

#[cfg(test)]
mod tests {
    use super::{StaticFileReloadStrategy, USubscriptionStaticFile};
    use std::collections::HashSet;
    use std::fs;
    use std::str::FromStr;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use up_rust::core::usubscription::{
        FetchSubscribersRequest, FetchSubscriptionsRequest, USubscription,
    };
    use up_rust::UUri;

    static TEST_FILE_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn write_static_config(contents: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        let counter = TEST_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        path.push(format!(
            "usubscription-static-file-test-{}-{}.json",
            std::process::id(),
            counter
        ));

        fs::write(&path, contents).expect("static test config written");
        path
    }

    #[test]
    fn uri_projection_key_owned_and_borrowed_conversion_match() {
        let uri = UUri {
            authority_name: "authority-a".to_string(),
            ue_id: 0x5BA0,
            ue_version_major: 0x1,
            resource_id: 0x8001,
            ..Default::default()
        };

        let key_from_borrowed = super::UriProjectionKey::from(&uri);
        let key_from_owned = super::UriProjectionKey::from(uri.clone());

        assert_eq!(key_from_owned, key_from_borrowed);
    }

    #[test]
    fn uri_projection_key_uses_canonical_major_and_resource_semantics() {
        let uri = UUri {
            authority_name: "authority-a".to_string(),
            ue_id: 0x5BA0,
            ue_version_major: 0x1FF,
            resource_id: 0x1_8001,
            ..Default::default()
        };

        let key = super::UriProjectionKey::from(&uri);

        assert_eq!(key.ue_version_major, uri.uentity_major_version());
        assert_eq!(key.resource_id, uri.resource_id());
        assert_eq!(key.ue_version_major, 0xFF);
        assert_eq!(key.resource_id, 0x8001);
    }

    #[tokio::test]
    async fn fetch_subscribers_dedupes_duplicate_subscribers_after_topic_normalization() {
        let static_path = write_static_config(
            r#"{
                "//authority-a/5BA0/1/8001": [
                    "//authority-b/5678/1/1234",
                    "//authority-b/5678/1/1234",
                    "//authority-c/5678/1/1234"
                ],
                "//authority-a/5BA0/1/8002": [
                    "//authority-z/5678/1/1234"
                ]
            }"#,
        );

        let backend = USubscriptionStaticFile::new(static_path.to_string_lossy().to_string());

        let response = backend
            .fetch_subscribers(FetchSubscribersRequest {
                topic: Some(UUri::from_str("//authority-a/5BA0/1/FFFF").expect("valid topic"))
                    .into(),
                ..Default::default()
            })
            .await
            .expect("fetch_subscribers should succeed");

        fs::remove_file(&static_path).expect("remove static config file");

        let subscriber_uris: HashSet<String> = response
            .subscribers
            .into_iter()
            .filter_map(|subscriber| subscriber.uri.into_option())
            .map(|subscriber_uri| subscriber_uri.to_uri(false))
            .collect();

        assert_eq!(subscriber_uris.len(), 3);
        assert!(subscriber_uris.contains("//authority-b/5678/1/1234"));
        assert!(subscriber_uris.contains("//authority-c/5678/1/1234"));
        assert!(subscriber_uris.contains("//authority-z/5678/1/1234"));
    }

    #[tokio::test]
    async fn cache_on_first_read_reuses_snapshot_until_cache_is_cleared() {
        let static_path = write_static_config(
            r#"{
                "//authority-a/5BA0/1/8001": ["//authority-b/5678/1/1234"]
            }"#,
        );
        let backend = USubscriptionStaticFile::with_reload_strategy(
            static_path.to_string_lossy().to_string(),
            StaticFileReloadStrategy::CacheOnFirstRead,
        );

        let first = backend
            .fetch_subscriptions(FetchSubscriptionsRequest::default())
            .await
            .expect("initial fetch_subscriptions should succeed");
        assert_eq!(first.subscriptions.len(), 1);

        fs::write(&static_path, r#"{}"#).expect("overwrite static config");

        let second = backend
            .fetch_subscriptions(FetchSubscriptionsRequest::default())
            .await
            .expect("cached fetch_subscriptions should succeed");
        assert_eq!(second.subscriptions.len(), 1);

        backend
            .clear_cached_subscriptions()
            .expect("cache clear should succeed");

        let third = backend
            .fetch_subscriptions(FetchSubscriptionsRequest::default())
            .await
            .expect("post-clear fetch_subscriptions should succeed");
        assert!(third.subscriptions.is_empty());

        fs::remove_file(&static_path).expect("remove static config file");
    }

    #[tokio::test]
    async fn always_reload_strategy_reads_updated_file_contents() {
        let static_path = write_static_config(
            r#"{
                "//authority-a/5BA0/1/8001": ["//authority-b/5678/1/1234"]
            }"#,
        );
        let backend = USubscriptionStaticFile::new(static_path.to_string_lossy().to_string());

        let first = backend
            .fetch_subscriptions(FetchSubscriptionsRequest::default())
            .await
            .expect("initial fetch_subscriptions should succeed");
        assert_eq!(first.subscriptions.len(), 1);

        fs::write(
            &static_path,
            r#"{
                "//authority-a/5BA0/1/8001": [
                    "//authority-b/5678/1/1234",
                    "//authority-c/5678/1/1234"
                ]
            }"#,
        )
        .expect("overwrite static config");

        let second = backend
            .fetch_subscriptions(FetchSubscriptionsRequest::default())
            .await
            .expect("reload fetch_subscriptions should succeed");
        assert_eq!(second.subscriptions.len(), 2);

        fs::remove_file(&static_path).expect("remove static config file");
    }
}

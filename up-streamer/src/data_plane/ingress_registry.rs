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

//! Ingress-route listener registry and lifecycle management.

use crate::control_plane::transport_identity::TransportIdentityKey;
use crate::data_plane::ingress_listener::IngressRouteListener;
use crate::observability::{events, fields};
use crate::routing::authority_filter::authority_to_wildcard_filter;
use crate::routing::publish_resolution::{
    PublishRouteResolver, PublishSourceFilterCacheKey, SourceFilterLookup,
};
use crate::routing::subscription_directory::SubscriptionDirectory;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fmt::{Debug, Display, Formatter};
use std::sync::Arc;
use tokio::sync::broadcast::Sender;
use tracing::{debug, warn};
use up_rust::{UMessage, UTransport, UUri};

const COMPONENT: &str = "ingress_registry";

/// Ingress route registration failures.
pub enum IngressRouteRegistrationError {
    FailedToRegisterRequestRouteListener,
    FailedToRegisterPublishRouteListener(UUri),
}

impl Debug for IngressRouteRegistrationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            IngressRouteRegistrationError::FailedToRegisterRequestRouteListener => {
                write!(f, "FailedToRegisterRequestRouteListener")
            }
            IngressRouteRegistrationError::FailedToRegisterPublishRouteListener(uri) => {
                write!(f, "FailedToRegisterPublishRouteListener({:?})", uri)
            }
        }
    }
}

impl Display for IngressRouteRegistrationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            IngressRouteRegistrationError::FailedToRegisterRequestRouteListener => {
                write!(
                    f,
                    "Failed to register notification request/response listener"
                )
            }
            IngressRouteRegistrationError::FailedToRegisterPublishRouteListener(uri) => {
                write!(
                    f,
                    "Failed to register publish route listener for URI: {}",
                    uri
                )
            }
        }
    }
}

impl Error for IngressRouteRegistrationError {}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
/// Identity for one ingress binding registration in the data plane.
struct IngressRouteBindingKey {
    transport: TransportIdentityKey,
    ingress_authority: String,
    egress_authority: String,
}

impl IngressRouteBindingKey {
    fn new(in_transport: Arc<dyn UTransport>, in_authority: &str, out_authority: &str) -> Self {
        Self {
            transport: TransportIdentityKey::new(in_transport),
            ingress_authority: in_authority.to_string(),
            egress_authority: out_authority.to_string(),
        }
    }
}

struct IngressRouteBinding {
    ref_count: usize,
    listener: Arc<IngressRouteListener>,
    registered_filters: Vec<ListenerFilter>,
}

type ListenerFilter = (UUri, Option<UUri>);

fn format_sink_filter(sink_filter: Option<&UUri>) -> String {
    sink_filter
        .map(fields::format_uri)
        .unwrap_or_else(|| fields::NONE.to_string())
}

fn unregister_event_name(sink_filter: Option<&UUri>, success: bool) -> &'static str {
    match (sink_filter.is_some(), success) {
        (true, true) => events::INGRESS_UNREGISTER_REQUEST_LISTENER_OK,
        (true, false) => events::INGRESS_UNREGISTER_REQUEST_LISTENER_FAILED,
        (false, true) => events::INGRESS_UNREGISTER_PUBLISH_LISTENER_OK,
        (false, false) => events::INGRESS_UNREGISTER_PUBLISH_LISTENER_FAILED,
    }
}

async fn rollback_registered_filters(
    in_transport: &Arc<dyn UTransport>,
    rollback_filters: &[ListenerFilter],
    route_listener: Arc<IngressRouteListener>,
    route_label: &str,
    in_authority: &str,
    out_authority: &str,
) {
    for (source_filter, sink_filter) in rollback_filters {
        let source_filter_value = fields::format_uri(source_filter);
        let sink_filter_value = format_sink_filter(sink_filter.as_ref());

        if let Err(err) = in_transport
            .unregister_listener(source_filter, sink_filter.as_ref(), route_listener.clone())
            .await
        {
            warn!(
                event = unregister_event_name(sink_filter.as_ref(), false),
                component = COMPONENT,
                route_label,
                in_authority,
                out_authority,
                source_filter = %source_filter_value,
                sink_filter = %sink_filter_value,
                err = %err,
                reason = "rollback_after_register_failure",
                "unable to unregister listener during rollback"
            );
        } else {
            debug!(
                event = unregister_event_name(sink_filter.as_ref(), true),
                component = COMPONENT,
                route_label,
                in_authority,
                out_authority,
                source_filter = %source_filter_value,
                sink_filter = %sink_filter_value,
                reason = "rollback_after_register_failure",
                "rollback listener unregister succeeded"
            );
        }
    }
}

/// Refcounted ingress listener registry keyed by route binding identity.
pub(crate) struct IngressRouteRegistry {
    bindings: tokio::sync::Mutex<HashMap<IngressRouteBindingKey, IngressRouteBinding>>,
    publish_filter_cache:
        tokio::sync::Mutex<HashMap<PublishSourceFilterCacheKey, SourceFilterLookup>>,
}

impl IngressRouteRegistry {
    /// Creates an empty ingress route registry.
    pub(crate) fn new() -> Self {
        Self {
            bindings: tokio::sync::Mutex::new(HashMap::new()),
            publish_filter_cache: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    async fn derive_publish_filters_cached(
        &self,
        in_authority: &str,
        out_authority: &str,
        subscription_directory: &SubscriptionDirectory,
    ) -> SourceFilterLookup {
        let (snapshot_version, subscribers) = subscription_directory
            .lookup_route_subscribers_with_version(out_authority)
            .await;
        let cache_key =
            PublishSourceFilterCacheKey::new(in_authority, out_authority, snapshot_version);

        if let Some(cached) = self
            .publish_filter_cache
            .lock()
            .await
            .get(&cache_key)
            .cloned()
        {
            return cached;
        }

        let derived =
            PublishRouteResolver::derive_source_filters(in_authority, out_authority, &subscribers);

        let mut publish_filter_cache = self.publish_filter_cache.lock().await;
        publish_filter_cache
            .entry(cache_key)
            .or_insert_with(|| derived.clone())
            .clone()
    }

    pub(crate) async fn register_route(
        &self,
        in_transport: Arc<dyn UTransport>,
        in_authority: &str,
        out_authority: &str,
        route_label: &str,
        out_sender: Sender<Arc<UMessage>>,
        subscription_directory: &SubscriptionDirectory,
    ) -> Result<Option<Arc<IngressRouteListener>>, IngressRouteRegistrationError> {
        let binding_key =
            IngressRouteBindingKey::new(in_transport.clone(), in_authority, out_authority);

        {
            let mut route_bindings = self.bindings.lock().await;
            if let Some(binding) = route_bindings.get_mut(&binding_key) {
                binding.ref_count += 1;
                return Ok(None);
            }
        }

        let route_listener = Arc::new(IngressRouteListener::new(route_label, out_sender.clone()));

        let mut rollback_filters: Vec<ListenerFilter> = Vec::new();

        let request_source_filter = authority_to_wildcard_filter(in_authority);
        let request_sink_filter = authority_to_wildcard_filter(out_authority);

        rollback_filters.push((
            request_source_filter.clone(),
            Some(request_sink_filter.clone()),
        ));

        if let Err(err) = in_transport
            .register_listener(
                &request_source_filter,
                Some(&request_sink_filter),
                route_listener.clone(),
            )
            .await
        {
            let request_source = fields::format_uri(&request_source_filter);
            let request_sink = fields::format_uri(&request_sink_filter);
            warn!(
                event = events::INGRESS_REGISTER_REQUEST_LISTENER_FAILED,
                component = COMPONENT,
                route_label,
                in_authority,
                out_authority,
                source_filter = %request_source,
                sink_filter = %request_sink,
                err = %err,
                "unable to register request listener"
            );
            rollback_registered_filters(
                &in_transport,
                &rollback_filters,
                route_listener.clone(),
                route_label,
                in_authority,
                out_authority,
            )
            .await;
            return Err(IngressRouteRegistrationError::FailedToRegisterRequestRouteListener);
        }

        let request_source = fields::format_uri(&request_source_filter);
        let request_sink = fields::format_uri(&request_sink_filter);
        debug!(
            event = events::INGRESS_REGISTER_REQUEST_LISTENER_OK,
            component = COMPONENT,
            route_label,
            in_authority,
            out_authority,
            source_filter = %request_source,
            sink_filter = %request_sink,
            "registered request listener"
        );

        let publish_source_filters = self
            .derive_publish_filters_cached(in_authority, out_authority, subscription_directory)
            .await;

        for source_uri in publish_source_filters.into_values() {
            let source_filter = fields::format_uri(&source_uri);

            if let Err(err) = in_transport
                .register_listener(&source_uri, None, route_listener.clone())
                .await
            {
                warn!(
                    event = events::INGRESS_REGISTER_PUBLISH_LISTENER_FAILED,
                    component = COMPONENT,
                    route_label,
                    in_authority,
                    out_authority,
                    source_filter = %source_filter,
                    sink_filter = fields::NONE,
                    err = %err,
                    "unable to register publish listener"
                );
                rollback_registered_filters(
                    &in_transport,
                    &rollback_filters,
                    route_listener.clone(),
                    route_label,
                    in_authority,
                    out_authority,
                )
                .await;
                return Err(
                    IngressRouteRegistrationError::FailedToRegisterPublishRouteListener(source_uri),
                );
            }

            rollback_filters.push((source_uri, None));
            debug!(
                event = events::INGRESS_REGISTER_PUBLISH_LISTENER_OK,
                component = COMPONENT,
                route_label,
                in_authority,
                out_authority,
                source_filter = %source_filter,
                sink_filter = fields::NONE,
                "registered publish listener"
            );
        }

        let mut route_bindings = self.bindings.lock().await;
        if let Some(binding) = route_bindings.get_mut(&binding_key) {
            binding.ref_count += 1;
            drop(route_bindings);
            rollback_registered_filters(
                &in_transport,
                &rollback_filters,
                route_listener,
                route_label,
                in_authority,
                out_authority,
            )
            .await;
            return Ok(None);
        }

        route_bindings.insert(
            binding_key,
            IngressRouteBinding {
                ref_count: 1,
                listener: route_listener.clone(),
                registered_filters: rollback_filters,
            },
        );
        Ok(Some(route_listener))
    }

    /// Unregisters ingress route listeners when the binding refcount reaches zero.
    pub(crate) async fn unregister_route(
        &self,
        in_transport: Arc<dyn UTransport>,
        in_authority: &str,
        out_authority: &str,
        _subscription_directory: &SubscriptionDirectory,
    ) {
        let binding_key =
            IngressRouteBindingKey::new(in_transport.clone(), in_authority, out_authority);

        let removed_binding = {
            let mut route_bindings = self.bindings.lock().await;

            let Some(binding) = route_bindings.get_mut(&binding_key) else {
                warn!(
                    event = events::INGRESS_UNREGISTER_REQUEST_LISTENER_FAILED,
                    component = COMPONENT,
                    route_label = fields::NONE,
                    in_authority,
                    out_authority,
                    source_filter = fields::NONE,
                    sink_filter = fields::NONE,
                    reason = "missing_transport_binding",
                    "unable to unregister route for missing transport binding"
                );
                return;
            };

            binding.ref_count -= 1;
            if binding.ref_count > 0 {
                return;
            }

            route_bindings.remove(&binding_key)
        };

        if let Some(binding) = removed_binding {
            let route_listener = binding.listener;
            for (source_filter, sink_filter) in binding.registered_filters {
                let source_filter_value = fields::format_uri(&source_filter);
                let sink_filter_value = format_sink_filter(sink_filter.as_ref());
                if let Err(err) = in_transport
                    .unregister_listener(
                        &source_filter,
                        sink_filter.as_ref(),
                        route_listener.clone(),
                    )
                    .await
                {
                    warn!(
                        event = unregister_event_name(sink_filter.as_ref(), false),
                        component = COMPONENT,
                        route_label = fields::NONE,
                        in_authority,
                        out_authority,
                        source_filter = %source_filter_value,
                        sink_filter = %sink_filter_value,
                        err = %err,
                        "unable to unregister ingress listener"
                    );
                } else {
                    debug!(
                        event = unregister_event_name(sink_filter.as_ref(), true),
                        component = COMPONENT,
                        route_label = fields::NONE,
                        in_authority,
                        out_authority,
                        source_filter = %source_filter_value,
                        sink_filter = %sink_filter_value,
                        "unregistered ingress listener"
                    );
                }
            }
        } else {
            warn!(
                event = events::INGRESS_UNREGISTER_REQUEST_LISTENER_FAILED,
                component = COMPONENT,
                route_label = fields::NONE,
                in_authority,
                out_authority,
                source_filter = fields::NONE,
                sink_filter = fields::NONE,
                reason = "missing_transport_binding",
                "route binding remove returned none"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::IngressRouteRegistry;
    use crate::routing::authority_filter::authority_to_wildcard_filter;
    use crate::routing::subscription_cache::SubscriptionCache;
    use crate::routing::subscription_directory::SubscriptionDirectory;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::str::FromStr;
    use std::sync::{Arc, Mutex as StdMutex};
    use up_rust::core::usubscription::{FetchSubscriptionsResponse, SubscriberInfo, Subscription};
    use up_rust::{UCode, UListener, UMessage, UStatus, UTransport, UUri};

    #[derive(Clone, Debug, Eq, PartialEq, Hash)]
    struct ListenerRegistration {
        source_filter: UUri,
        sink_filter: Option<UUri>,
    }

    fn listener_registration(
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
    ) -> ListenerRegistration {
        ListenerRegistration {
            source_filter: source_filter.clone(),
            sink_filter: sink_filter.cloned(),
        }
    }

    #[derive(Default)]
    struct RecordingTransport {
        register_call_counts: StdMutex<HashMap<ListenerRegistration, usize>>,
        unregister_call_counts: StdMutex<HashMap<ListenerRegistration, usize>>,
    }

    impl RecordingTransport {
        fn register_call_count(&self, source_filter: &UUri, sink_filter: Option<&UUri>) -> usize {
            self.register_call_counts
                .lock()
                .expect("lock register_call_counts")
                .get(&listener_registration(source_filter, sink_filter))
                .copied()
                .unwrap_or(0)
        }

        fn unregister_call_count(&self, source_filter: &UUri, sink_filter: Option<&UUri>) -> usize {
            self.unregister_call_counts
                .lock()
                .expect("lock unregister_call_counts")
                .get(&listener_registration(source_filter, sink_filter))
                .copied()
                .unwrap_or(0)
        }
    }

    #[async_trait]
    impl UTransport for RecordingTransport {
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
            source_filter: &UUri,
            sink_filter: Option<&UUri>,
            _listener: Arc<dyn UListener>,
        ) -> Result<(), UStatus> {
            let registration = listener_registration(source_filter, sink_filter);
            let mut counts = self
                .register_call_counts
                .lock()
                .expect("lock register_call_counts");
            let entry = counts.entry(registration).or_insert(0);
            *entry += 1;
            Ok(())
        }

        async fn unregister_listener(
            &self,
            source_filter: &UUri,
            sink_filter: Option<&UUri>,
            _listener: Arc<dyn UListener>,
        ) -> Result<(), UStatus> {
            let registration = listener_registration(source_filter, sink_filter);
            let mut counts = self
                .unregister_call_counts
                .lock()
                .expect("lock unregister_call_counts");
            let entry = counts.entry(registration).or_insert(0);
            *entry += 1;
            Ok(())
        }
    }

    fn make_subscription_cache(entries: &[(&str, &str)]) -> SubscriptionCache {
        let subscriptions = entries
            .iter()
            .map(|(topic, subscriber)| Subscription {
                topic: Some(UUri::from_str(topic).expect("valid topic UUri")).into(),
                subscriber: Some(SubscriberInfo {
                    uri: Some(UUri::from_str(subscriber).expect("valid subscriber UUri")).into(),
                    ..Default::default()
                })
                .into(),
                ..Default::default()
            })
            .collect();

        SubscriptionCache::new(FetchSubscriptionsResponse {
            subscriptions,
            ..Default::default()
        })
        .expect("valid subscription cache")
    }

    fn subscription_snapshot(entries: &[(&str, &str)]) -> FetchSubscriptionsResponse {
        let subscriptions = entries
            .iter()
            .map(|(topic, subscriber)| Subscription {
                topic: Some(UUri::from_str(topic).expect("valid topic UUri")).into(),
                subscriber: Some(SubscriberInfo {
                    uri: Some(UUri::from_str(subscriber).expect("valid subscriber UUri")).into(),
                    ..Default::default()
                })
                .into(),
                ..Default::default()
            })
            .collect();

        FetchSubscriptionsResponse {
            subscriptions,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn register_and_unregister_route_registers_and_unregisters_request_and_publish_filters() {
        let route_registry = IngressRouteRegistry::new();
        let recording_transport = Arc::new(RecordingTransport::default());
        let in_transport: Arc<dyn UTransport> = recording_transport.clone();
        let (out_sender, _) = tokio::sync::broadcast::channel(16);
        let subscription_cache =
            make_subscription_cache(&[("//authority-a/5BA0/1/8001", "//authority-b/5678/1/1234")]);
        let subscription_directory = SubscriptionDirectory::new(subscription_cache);

        assert!(route_registry
            .register_route(
                in_transport.clone(),
                "authority-a",
                "authority-b",
                "test-route",
                out_sender,
                &subscription_directory,
            )
            .await
            .is_ok());

        route_registry
            .unregister_route(
                in_transport,
                "authority-a",
                "authority-b",
                &subscription_directory,
            )
            .await;

        let request_source = authority_to_wildcard_filter("authority-a");
        let request_sink = authority_to_wildcard_filter("authority-b");
        let publish_source =
            UUri::try_from_parts("authority-a", 0x5BA0, 0x1, 0x8001).expect("valid publish source");

        assert_eq!(
            recording_transport.register_call_count(&request_source, Some(&request_sink)),
            1
        );
        assert_eq!(
            recording_transport.register_call_count(&publish_source, None),
            1
        );
        assert_eq!(
            recording_transport.unregister_call_count(&request_source, Some(&request_sink)),
            1
        );
        assert_eq!(
            recording_transport.unregister_call_count(&publish_source, None),
            1
        );
    }

    #[tokio::test]
    async fn duplicate_register_route_keeps_single_listener_registration() {
        let route_registry = IngressRouteRegistry::new();
        let recording_transport = Arc::new(RecordingTransport::default());
        let in_transport: Arc<dyn UTransport> = recording_transport.clone();
        let (out_sender, _) = tokio::sync::broadcast::channel(16);
        let subscription_cache =
            make_subscription_cache(&[("//authority-a/5BA0/1/8001", "//authority-b/5678/1/1234")]);
        let subscription_directory = SubscriptionDirectory::new(subscription_cache);

        let first_register = route_registry
            .register_route(
                in_transport.clone(),
                "authority-a",
                "authority-b",
                "test-route",
                out_sender.clone(),
                &subscription_directory,
            )
            .await
            .expect("first register success");
        let second_register = route_registry
            .register_route(
                in_transport,
                "authority-a",
                "authority-b",
                "test-route",
                out_sender,
                &subscription_directory,
            )
            .await
            .expect("second register success");

        assert!(first_register.is_some());
        assert!(second_register.is_none());

        let request_source = authority_to_wildcard_filter("authority-a");
        let request_sink = authority_to_wildcard_filter("authority-b");
        let publish_source =
            UUri::try_from_parts("authority-a", 0x5BA0, 0x1, 0x8001).expect("valid publish source");

        assert_eq!(
            recording_transport.register_call_count(&request_source, Some(&request_sink)),
            1
        );
        assert_eq!(
            recording_transport.register_call_count(&publish_source, None),
            1
        );
    }

    #[tokio::test]
    async fn unregister_route_uses_registered_filters_after_snapshot_change() {
        let route_registry = IngressRouteRegistry::new();
        let recording_transport = Arc::new(RecordingTransport::default());
        let in_transport: Arc<dyn UTransport> = recording_transport.clone();
        let (out_sender, _) = tokio::sync::broadcast::channel(16);
        let subscription_cache =
            make_subscription_cache(&[("//authority-a/5BA0/1/8001", "//authority-b/5678/1/1234")]);
        let subscription_directory = SubscriptionDirectory::new(subscription_cache);

        route_registry
            .register_route(
                in_transport.clone(),
                "authority-a",
                "authority-b",
                "test-route",
                out_sender,
                &subscription_directory,
            )
            .await
            .expect("route register should succeed");

        subscription_directory
            .apply_snapshot(subscription_snapshot(&[(
                "//authority-a/5BA1/1/8002",
                "//authority-b/5678/1/1234",
            )]))
            .await
            .expect("snapshot update should succeed");

        route_registry
            .unregister_route(
                in_transport,
                "authority-a",
                "authority-b",
                &subscription_directory,
            )
            .await;

        let request_source = authority_to_wildcard_filter("authority-a");
        let request_sink = authority_to_wildcard_filter("authority-b");
        let registered_publish_source =
            UUri::try_from_parts("authority-a", 0x5BA0, 0x1, 0x8001).expect("valid publish source");
        let updated_publish_source =
            UUri::try_from_parts("authority-a", 0x5BA1, 0x1, 0x8002).expect("valid publish source");

        assert_eq!(
            recording_transport.unregister_call_count(&request_source, Some(&request_sink)),
            1
        );
        assert_eq!(
            recording_transport.unregister_call_count(&registered_publish_source, None),
            1
        );
        assert_eq!(
            recording_transport.unregister_call_count(&updated_publish_source, None),
            0
        );
    }

    #[tokio::test]
    async fn register_route_cache_key_uses_snapshot_version() {
        let route_registry = IngressRouteRegistry::new();
        let recording_transport = Arc::new(RecordingTransport::default());
        let in_transport: Arc<dyn UTransport> = recording_transport.clone();
        let (out_sender, _) = tokio::sync::broadcast::channel(16);
        let subscription_cache =
            make_subscription_cache(&[("//authority-a/5BA0/1/8001", "//authority-b/5678/1/1234")]);
        let subscription_directory = SubscriptionDirectory::new(subscription_cache);

        route_registry
            .register_route(
                in_transport.clone(),
                "authority-a",
                "authority-b",
                "test-route",
                out_sender.clone(),
                &subscription_directory,
            )
            .await
            .expect("first route register should succeed");

        route_registry
            .unregister_route(
                in_transport.clone(),
                "authority-a",
                "authority-b",
                &subscription_directory,
            )
            .await;

        subscription_directory
            .apply_snapshot(subscription_snapshot(&[(
                "//authority-a/5BA1/1/8002",
                "//authority-b/5678/1/1234",
            )]))
            .await
            .expect("snapshot update should succeed");

        route_registry
            .register_route(
                in_transport,
                "authority-a",
                "authority-b",
                "test-route",
                out_sender,
                &subscription_directory,
            )
            .await
            .expect("second route register should succeed");

        let first_publish_source =
            UUri::try_from_parts("authority-a", 0x5BA0, 0x1, 0x8001).expect("valid publish source");
        let second_publish_source =
            UUri::try_from_parts("authority-a", 0x5BA1, 0x1, 0x8002).expect("valid publish source");

        assert_eq!(
            recording_transport.register_call_count(&first_publish_source, None),
            1
        );
        assert_eq!(
            recording_transport.register_call_count(&second_publish_source, None),
            1
        );
    }
}

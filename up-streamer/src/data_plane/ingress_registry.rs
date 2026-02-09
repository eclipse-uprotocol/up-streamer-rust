//! Ingress-route listener registry and lifecycle management.

use crate::control_plane::transport_identity::TransportIdentityKey;
use crate::data_plane::ingress_listener::IngressRouteListener;
use crate::routing::authority_filter::authority_to_wildcard_filter;
use crate::routing::publish_resolution::PublishRouteResolver;
use crate::routing::subscription_directory::SubscriptionDirectory;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::fmt::{Debug, Display, Formatter};
use std::sync::Arc;
use tokio::sync::broadcast::Sender;
use tracing::{debug, info, warn};
use up_rust::{UMessage, UTransport, UUri};

const INGRESS_ROUTE_REGISTRY_TAG: &str = "IngressRouteRegistry:";
const INGRESS_ROUTE_REGISTRY_FN_REGISTER_ROUTE_TAG: &str = "register_route:";
const INGRESS_ROUTE_REGISTRY_FN_UNREGISTER_ROUTE_TAG: &str = "unregister_route:";

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
}

type ListenerFilter = (UUri, Option<UUri>);

#[allow(clippy::mutable_key_type)]
async fn rollback_registered_filters(
    in_transport: &Arc<dyn UTransport>,
    rollback_filters: &HashSet<ListenerFilter>,
    route_listener: Arc<IngressRouteListener>,
) {
    for (source_filter, sink_filter) in rollback_filters {
        if let Err(err) = in_transport
            .unregister_listener(source_filter, sink_filter.as_ref(), route_listener.clone())
            .await
        {
            warn!(
                "{}:{} unable to unregister listener, error: {}",
                INGRESS_ROUTE_REGISTRY_TAG, INGRESS_ROUTE_REGISTRY_FN_REGISTER_ROUTE_TAG, err
            );
        }
    }
}

/// Refcounted ingress listener registry keyed by route binding identity.
pub(crate) struct IngressRouteRegistry {
    bindings: tokio::sync::Mutex<HashMap<IngressRouteBindingKey, IngressRouteBinding>>,
}

impl IngressRouteRegistry {
    /// Creates an empty ingress route registry.
    pub(crate) fn new() -> Self {
        Self {
            bindings: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    pub(crate) async fn register_route(
        &self,
        in_transport: Arc<dyn UTransport>,
        in_authority: &str,
        out_authority: &str,
        route_id: &str,
        out_sender: Sender<Arc<UMessage>>,
        subscription_directory: &SubscriptionDirectory,
    ) -> Result<Option<Arc<IngressRouteListener>>, IngressRouteRegistrationError> {
        let binding_key =
            IngressRouteBindingKey::new(in_transport.clone(), in_authority, out_authority);
        let mut route_bindings = self.bindings.lock().await;

        if let Some(binding) = route_bindings.get_mut(&binding_key) {
            binding.ref_count += 1;
            if binding.ref_count > 1 {
                return Ok(None);
            }
            return Ok(Some(binding.listener.clone()));
        }

        let route_listener = Arc::new(IngressRouteListener::new(route_id, out_sender.clone()));

        #[allow(clippy::mutable_key_type)]
        let mut rollback_filters: HashSet<ListenerFilter> = HashSet::new();

        let request_source_filter = authority_to_wildcard_filter(in_authority);
        let request_sink_filter = authority_to_wildcard_filter(out_authority);

        rollback_filters.insert((
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
            warn!(
                "{}:{} unable to register request listener, error: {}",
                INGRESS_ROUTE_REGISTRY_TAG, INGRESS_ROUTE_REGISTRY_FN_REGISTER_ROUTE_TAG, err
            );
            rollback_registered_filters(&in_transport, &rollback_filters, route_listener.clone())
                .await;
            return Err(IngressRouteRegistrationError::FailedToRegisterRequestRouteListener);
        }

        debug!(
            "{}:{} able to register request listener",
            INGRESS_ROUTE_REGISTRY_TAG, INGRESS_ROUTE_REGISTRY_FN_REGISTER_ROUTE_TAG
        );

        #[allow(clippy::mutable_key_type)]
        let subscribers = subscription_directory
            .lookup_route_subscribers(
                out_authority,
                INGRESS_ROUTE_REGISTRY_TAG,
                INGRESS_ROUTE_REGISTRY_FN_REGISTER_ROUTE_TAG,
            )
            .await;

        let route_resolver = PublishRouteResolver::new(
            in_authority,
            out_authority,
            INGRESS_ROUTE_REGISTRY_TAG,
            INGRESS_ROUTE_REGISTRY_FN_REGISTER_ROUTE_TAG,
        );
        #[allow(clippy::mutable_key_type)]
        let publish_source_filters = route_resolver.derive_source_filters(&subscribers);

        for source_uri in publish_source_filters {
            info!(
                "in authority: {}, out authority: {}, source URI filter: {:?}",
                in_authority, out_authority, source_uri
            );

            if let Err(err) = in_transport
                .register_listener(&source_uri, None, route_listener.clone())
                .await
            {
                warn!(
                    "{}:{} unable to register listener, error: {}",
                    INGRESS_ROUTE_REGISTRY_TAG, INGRESS_ROUTE_REGISTRY_FN_REGISTER_ROUTE_TAG, err
                );
                rollback_registered_filters(
                    &in_transport,
                    &rollback_filters,
                    route_listener.clone(),
                )
                .await;
                return Err(
                    IngressRouteRegistrationError::FailedToRegisterPublishRouteListener(source_uri),
                );
            }

            rollback_filters.insert((source_uri, None));
            debug!("{INGRESS_ROUTE_REGISTRY_TAG}:{INGRESS_ROUTE_REGISTRY_FN_REGISTER_ROUTE_TAG} able to register listener");
        }

        route_bindings.insert(
            binding_key,
            IngressRouteBinding {
                ref_count: 1,
                listener: route_listener.clone(),
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
        subscription_directory: &SubscriptionDirectory,
    ) {
        let binding_key =
            IngressRouteBindingKey::new(in_transport.clone(), in_authority, out_authority);

        let mut route_bindings = self.bindings.lock().await;

        let active_num = {
            let Some(binding) = route_bindings.get_mut(&binding_key) else {
                warn!(
                    "{INGRESS_ROUTE_REGISTRY_TAG}:{INGRESS_ROUTE_REGISTRY_FN_UNREGISTER_ROUTE_TAG} no such in_transport_key, out_authority: {out_authority:?}"
                );
                return;
            };
            binding.ref_count -= 1;
            binding.ref_count
        };

        if active_num == 0 {
            let removed = route_bindings.remove(&binding_key);
            if let Some(binding) = removed {
                let route_listener = binding.listener;
                let request_source_filter = authority_to_wildcard_filter(in_authority);
                let request_sink_filter = authority_to_wildcard_filter(out_authority);

                let request_unreg_res = in_transport
                    .unregister_listener(
                        &request_source_filter,
                        Some(&request_sink_filter),
                        route_listener.clone(),
                    )
                    .await;

                if let Err(err) = request_unreg_res {
                    warn!("{INGRESS_ROUTE_REGISTRY_TAG}:{INGRESS_ROUTE_REGISTRY_FN_UNREGISTER_ROUTE_TAG} unable to unregister request listener, error: {err}");
                } else {
                    debug!("{INGRESS_ROUTE_REGISTRY_TAG}:{INGRESS_ROUTE_REGISTRY_FN_UNREGISTER_ROUTE_TAG} able to unregister request listener");
                }

                #[allow(clippy::mutable_key_type)]
                let subscribers = subscription_directory
                    .lookup_route_subscribers(
                        out_authority,
                        INGRESS_ROUTE_REGISTRY_TAG,
                        INGRESS_ROUTE_REGISTRY_FN_UNREGISTER_ROUTE_TAG,
                    )
                    .await;

                let route_resolver = PublishRouteResolver::new(
                    in_authority,
                    out_authority,
                    INGRESS_ROUTE_REGISTRY_TAG,
                    INGRESS_ROUTE_REGISTRY_FN_UNREGISTER_ROUTE_TAG,
                );
                #[allow(clippy::mutable_key_type)]
                let publish_source_filters = route_resolver.derive_source_filters(&subscribers);

                for source_uri in publish_source_filters {
                    if let Err(err) = in_transport
                        .unregister_listener(&source_uri, None, route_listener.clone())
                        .await
                    {
                        warn!("{INGRESS_ROUTE_REGISTRY_TAG}:{INGRESS_ROUTE_REGISTRY_FN_UNREGISTER_ROUTE_TAG} unable to unregister publish listener, error: {err}");
                    } else {
                        debug!("{INGRESS_ROUTE_REGISTRY_TAG}:{INGRESS_ROUTE_REGISTRY_FN_UNREGISTER_ROUTE_TAG} able to unregister publish listener");
                    }
                }
            } else {
                warn!("{INGRESS_ROUTE_REGISTRY_TAG}:{INGRESS_ROUTE_REGISTRY_FN_UNREGISTER_ROUTE_TAG} none found we can remove, out_authority: {out_authority:?}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::IngressRouteRegistry;
    use crate::routing::authority_filter::authority_to_wildcard_filter;
    use crate::routing::subscription_directory::SubscriptionDirectory;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::str::FromStr;
    use std::sync::{Arc, Mutex as StdMutex};
    use subscription_cache::SubscriptionCache;
    use tokio::sync::Mutex;
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

    fn make_subscription_cache(entries: &[(&str, &str)]) -> Arc<Mutex<SubscriptionCache>> {
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

        Arc::new(Mutex::new(
            SubscriptionCache::new(FetchSubscriptionsResponse {
                subscriptions,
                ..Default::default()
            })
            .expect("valid subscription cache"),
        ))
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
}

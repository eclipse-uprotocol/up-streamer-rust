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

use crate::endpoint::Endpoint;
use async_trait::async_trait;
use lazy_static::lazy_static;
use log::*;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::fmt::{Debug, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::str;
use std::sync::Arc;
use std::thread;
use subscription_cache::SubscriptionCache;
use tokio::runtime::Builder;
use tokio::runtime::Runtime;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::sync::Mutex;
use tokio::task;
use up_rust::core::usubscription::{FetchSubscriptionsRequest, SubscriberInfo, USubscription};
use up_rust::{UCode, UListener, UMessage, UPayloadFormat, UStatus, UTransport, UUri, UUID};

const USTREAMER_TAG: &str = "UStreamer:";
const USTREAMER_FN_NEW_TAG: &str = "new():";
const USTREAMER_FN_ADD_FORWARDING_RULE_TAG: &str = "add_forwarding_rule():";
const USTREAMER_FN_DELETE_FORWARDING_RULE_TAG: &str = "delete_forwarding_rule():";

const THREAD_NUM: usize = 10;

// Create a separate tokio Runtime for running the callback
lazy_static! {
    static ref CB_RUNTIME: Runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(THREAD_NUM)
        .enable_all()
        .build()
        .expect("Unable to create callback runtime");
}

fn uauthority_to_uuri(authority_name: &str) -> UUri {
    // let mut uuri = UUri::any();
    // uuri.authority_name = authority_name.to_string();
    // uuri

    UUri {
        authority_name: authority_name.to_string(),
        ue_id: 0xFFFF_FFFF,     // any instance, any service
        ue_version_major: 0xFF, // any
        resource_id: 0xFFFF,    // any
        ..Default::default()
    }
}

// Used to track any errors in creating forwarding listeners
pub enum ForwardingListenerError {
    FailToRegisterNotificationRequestResponseListener,
    FailToRegisterPublishListener(UUri), // we can embed the subscriber.topic here which failed
}

impl Debug for ForwardingListenerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ForwardingListenerError::FailToRegisterNotificationRequestResponseListener => {
                write!(f, "FailToRegisterNotificationRequestResponseListener")
            }
            ForwardingListenerError::FailToRegisterPublishListener(uri) => {
                write!(f, "FailToRegisterPublishListener({:?})", uri)
            }
        }
    }
}

impl Display for ForwardingListenerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ForwardingListenerError::FailToRegisterNotificationRequestResponseListener => {
                write!(
                    f,
                    "Failed to register notification request/response listener"
                )
            }
            ForwardingListenerError::FailToRegisterPublishListener(uri) => {
                write!(f, "Failed to register publish listener for URI: {}", uri)
            }
        }
    }
}

impl Error for ForwardingListenerError {}

// the 'gatekeeper' which will prevent us from erroneously being able to add duplicate
// forwarding rules or delete those rules which don't exist
type ForwardingRules = Mutex<HashSet<(String, String, ComparableTransport, ComparableTransport)>>;

const TRANSPORT_FORWARDERS_TAG: &str = "TransportForwarders:";
const TRANSPORT_FORWARDERS_FN_INSERT_TAG: &str = "insert:";
const TRANSPORT_FORWARDERS_FN_REMOVE_TAG: &str = "remove:";

type TransportForwardersContainer =
    Mutex<HashMap<ComparableTransport, (usize, Arc<TransportForwarder>, Sender<Arc<UMessage>>)>>;

// we only need one TransportForwarder per out `UTransport`, so we keep track of that one here
// and the Sender necessary to hand off to the listener for the in `UTransport`
struct TransportForwarders {
    message_queue_size: usize,
    forwarders: TransportForwardersContainer,
}

impl TransportForwarders {
    pub fn new(message_queue_size: usize) -> Self {
        Self {
            message_queue_size,
            forwarders: Mutex::new(HashMap::new()),
        }
    }

    pub async fn insert(&mut self, out_transport: Arc<dyn UTransport>) -> Sender<Arc<UMessage>> {
        let out_comparable_transport = ComparableTransport::new(out_transport.clone());

        let mut transport_forwarders = self.forwarders.lock().await;

        let (active, _, sender) = transport_forwarders
            .entry(out_comparable_transport)
            .or_insert_with(|| {
                debug!(
                    "{TRANSPORT_FORWARDERS_TAG}:{TRANSPORT_FORWARDERS_FN_INSERT_TAG} Inserting..."
                );
                let (tx, rx) = tokio::sync::broadcast::channel(self.message_queue_size);
                (0, Arc::new(TransportForwarder::new(out_transport, rx)), tx)
            });
        *active += 1;
        sender.clone()
    }

    pub async fn remove(&mut self, out_transport: Arc<dyn UTransport>) {
        let out_comparable_transport = ComparableTransport::new(out_transport.clone());

        let mut transport_forwarders = self.forwarders.lock().await;

        let active_num = {
            let Some((active, _, _)) = transport_forwarders.get_mut(&out_comparable_transport)
            else {
                warn!("{TRANSPORT_FORWARDERS_TAG}:{TRANSPORT_FORWARDERS_FN_REMOVE_TAG} no such out_comparable_transport");
                return;
            };

            *active -= 1;
            *active
        };

        if active_num == 0 {
            let removed = transport_forwarders.remove(&out_comparable_transport);
            debug!("{TRANSPORT_FORWARDERS_TAG}:{TRANSPORT_FORWARDERS_FN_REMOVE_TAG} went to remove TransportForwarder for this transport");
            if removed.is_none() {
                warn!("{TRANSPORT_FORWARDERS_TAG}:{TRANSPORT_FORWARDERS_FN_REMOVE_TAG} was none to remove");
            } else {
                debug!("{TRANSPORT_FORWARDERS_TAG}:{TRANSPORT_FORWARDERS_FN_REMOVE_TAG} had one to remove");
            }
        }
    }
}

const FORWARDING_LISTENERS_TAG: &str = "ForwardingListeners:";
const FORWARDING_LISTENERS_FN_INSERT_TAG: &str = "insert:";
const FORWARDING_LISTENERS_FN_REMOVE_TAG: &str = "remove:";

type ForwardingListenersContainer =
    Mutex<HashMap<(ComparableTransport, String, String), (usize, Arc<ForwardingListener>)>>;

// we must have only a single listener per in UTransport and out UAuthority
struct ForwardingListeners {
    listeners: ForwardingListenersContainer,
}

impl ForwardingListeners {
    pub fn new() -> Self {
        Self {
            listeners: Mutex::new(HashMap::new()),
        }
    }

    fn publish_source_uri_for_rule(
        in_authority: &str,
        out_authority: &str,
        topic: &UUri,
        action: &str,
    ) -> Option<UUri> {
        if topic.authority_name != "*" && topic.authority_name != in_authority {
            debug!(
                "{FORWARDING_LISTENERS_TAG}:{action} skipping publish listener {action} for in_authority='{in_authority}', out_authority='{out_authority}', topic_authority='{}', topic={topic:?}",
                topic.authority_name
            );
            return None;
        }

        match UUri::try_from_parts(
            in_authority,
            topic.ue_id,
            topic.uentity_major_version(),
            topic.resource_id(),
        ) {
            Ok(source_uri) => Some(source_uri),
            Err(err) => {
                warn!(
                    "{FORWARDING_LISTENERS_TAG}:{action} unable to build publish source URI for in_authority='{in_authority}', out_authority='{out_authority}', topic={topic:?}: {err}"
                );
                None
            }
        }
    }

    pub async fn insert(
        &self,
        in_transport: Arc<dyn UTransport>,
        in_authority: &str,
        out_authority: &str,
        forwarding_id: &str,
        out_sender: Sender<Arc<UMessage>>,
        subscription_cache: Arc<Mutex<SubscriptionCache>>,
    ) -> Result<Option<Arc<ForwardingListener>>, ForwardingListenerError> {
        let in_comparable_transport = ComparableTransport::new(in_transport.clone());
        let mut forwarding_listeners = self.listeners.lock().await;

        if let Some((active, forwarding_listener)) = forwarding_listeners.get_mut(&(
            in_comparable_transport.clone(),
            in_authority.to_string(),
            out_authority.to_string(),
        )) {
            *active += 1;
            if *active > 1 {
                return Ok(None);
            } else {
                return Ok(Some(forwarding_listener.clone()));
            }
        }

        let forwarding_listener =
            Arc::new(ForwardingListener::new(forwarding_id, out_sender.clone()));

        type SourceSinkFilterPair = (UUri, Option<UUri>);
        #[allow(clippy::mutable_key_type)]
        let mut uuris_to_backpedal: HashSet<SourceSinkFilterPair> = HashSet::new();

        let request_source_filter = uauthority_to_uuri(in_authority);
        let request_sink_filter = uauthority_to_uuri(out_authority);

        // Perform async registration and fetching
        uuris_to_backpedal.insert((
            request_source_filter.clone(),
            Some(request_sink_filter.clone()),
        ));

        if let Err(err) = in_transport
            .register_listener(
                &request_source_filter,
                Some(&request_sink_filter),
                forwarding_listener.clone(),
            )
            .await
        {
            warn!(
                "{}:{} unable to register request listener, error: {}",
                FORWARDING_LISTENERS_TAG, FORWARDING_LISTENERS_FN_INSERT_TAG, err
            );
            for uuri_pair in &uuris_to_backpedal {
                if let Err(err) = in_transport
                    .unregister_listener(
                        &uuri_pair.0,
                        uuri_pair.1.as_ref(),
                        forwarding_listener.clone(),
                    )
                    .await
                {
                    warn!(
                        "{}:{} unable to unregister listener, error: {}",
                        FORWARDING_LISTENERS_TAG, FORWARDING_LISTENERS_FN_INSERT_TAG, err
                    );
                };
            }
            return Err(ForwardingListenerError::FailToRegisterNotificationRequestResponseListener);
        } else {
            debug!(
                "{}:{} able to register request listener",
                FORWARDING_LISTENERS_TAG, FORWARDING_LISTENERS_FN_INSERT_TAG
            );
        }

        #[allow(clippy::mutable_key_type)]
        let subscribers = match subscription_cache
            .lock()
            .await
            .fetch_cache_entry(out_authority.into())
        {
            Some(subscribers) => subscribers,
            None => {
                warn!(
                    "{}:{} no subscribers found for out_authority: {:?}",
                    FORWARDING_LISTENERS_TAG, FORWARDING_LISTENERS_FN_INSERT_TAG, out_authority
                );
                HashSet::new()
            }
        };

        for subscriber in subscribers {
            let Some(source_uri) = Self::publish_source_uri_for_rule(
                in_authority,
                out_authority,
                &subscriber.topic,
                FORWARDING_LISTENERS_FN_INSERT_TAG,
            ) else {
                continue;
            };

            info!(
                "in authority: {}, out authority: {}, source URI filter: {:?}",
                in_authority, out_authority, source_uri
            );

            if let Err(err) = in_transport
                .register_listener(&source_uri, None, forwarding_listener.clone())
                .await
            {
                warn!(
                    "{}:{} unable to register listener, error: {}",
                    FORWARDING_LISTENERS_TAG, FORWARDING_LISTENERS_FN_INSERT_TAG, err
                );
                // Perform async unregister_listener
                for uuri_pair in &uuris_to_backpedal {
                    if let Err(err) = in_transport
                        .unregister_listener(
                            &uuri_pair.0,
                            uuri_pair.1.as_ref(),
                            forwarding_listener.clone(),
                        )
                        .await
                    {
                        warn!(
                            "{}:{} unable to unregister listener, error: {}",
                            FORWARDING_LISTENERS_TAG, FORWARDING_LISTENERS_FN_INSERT_TAG, err
                        );
                    };
                }
                return Err(ForwardingListenerError::FailToRegisterPublishListener(
                    subscriber.topic,
                ));
            } else {
                uuris_to_backpedal.insert((source_uri, None));
                debug!("{FORWARDING_LISTENERS_TAG}:{FORWARDING_LISTENERS_FN_INSERT_TAG} able to register listener");
            }
        }

        // Insert the new listener and update the active count
        forwarding_listeners.insert(
            (
                in_comparable_transport,
                in_authority.to_string(),
                out_authority.to_string(),
            ),
            (1, forwarding_listener.clone()),
        );
        Ok(Some(forwarding_listener))
    }

    pub async fn remove(
        &self,
        in_transport: Arc<dyn UTransport>,
        in_authority: &str,
        out_authority: &str,
        subscription_cache: Arc<Mutex<SubscriptionCache>>,
    ) {
        let in_comparable_transport = ComparableTransport::new(in_transport.clone());

        let mut forwarding_listeners = self.listeners.lock().await;

        let active_num = {
            let Some((active, _)) = forwarding_listeners.get_mut(&(
                in_comparable_transport.clone(),
                in_authority.to_string(),
                out_authority.to_string(),
            )) else {
                warn!("{FORWARDING_LISTENERS_TAG}:{FORWARDING_LISTENERS_FN_REMOVE_TAG} no such out_comparable_transport, out_authority: {out_authority:?}");
                return;
            };
            *active -= 1;
            *active
        };

        if active_num == 0 {
            let removed = forwarding_listeners.remove(&(
                in_comparable_transport,
                in_authority.to_string(),
                out_authority.to_string(),
            ));
            if let Some((_, forwarding_listener)) = removed {
                let request_source_filter = uauthority_to_uuri(in_authority);
                let request_sink_filter = uauthority_to_uuri(out_authority);

                let request_unreg_res = in_transport
                    .unregister_listener(
                        &request_source_filter,
                        Some(&request_sink_filter),
                        forwarding_listener.clone(),
                    )
                    .await;

                if let Err(err) = request_unreg_res {
                    warn!("{FORWARDING_LISTENERS_TAG}:{FORWARDING_LISTENERS_FN_REMOVE_TAG} unable to unregister request listener, error: {err}");
                } else {
                    debug!("{FORWARDING_LISTENERS_TAG}:{FORWARDING_LISTENERS_FN_REMOVE_TAG} able to unregister request listener");
                }

                #[allow(clippy::mutable_key_type)]
                let subscribers = match subscription_cache
                    .lock()
                    .await
                    .fetch_cache_entry(out_authority.into())
                {
                    Some(subscribers) => subscribers,
                    None => {
                        warn!(
                            "{}:{} no subscribers found for out_authority: {:?}",
                            FORWARDING_LISTENERS_TAG,
                            FORWARDING_LISTENERS_FN_REMOVE_TAG,
                            out_authority
                        );
                        HashSet::new()
                    }
                };

                for subscriber in subscribers {
                    let Some(source_uri) = Self::publish_source_uri_for_rule(
                        in_authority,
                        out_authority,
                        &subscriber.topic,
                        FORWARDING_LISTENERS_FN_REMOVE_TAG,
                    ) else {
                        continue;
                    };

                    if let Err(err) = in_transport
                        .unregister_listener(&source_uri, None, forwarding_listener.clone())
                        .await
                    {
                        warn!("{FORWARDING_LISTENERS_TAG}:{FORWARDING_LISTENERS_FN_REMOVE_TAG} unable to unregister publish listener, error: {err}");
                    } else {
                        debug!("{FORWARDING_LISTENERS_TAG}:{FORWARDING_LISTENERS_FN_REMOVE_TAG} able to unregister publish listener");
                    }
                }
            } else {
                warn!("{FORWARDING_LISTENERS_TAG}:{FORWARDING_LISTENERS_FN_REMOVE_TAG} none found we can remove, out_authority: {out_authority:?}");
            }
        }
    }
}

/// A [`UStreamer`] is used to coordinate the addition and deletion of forwarding rules between
/// [`Endpoint`][crate::Endpoint]s
///
/// Essentially, it's a means of setting up rules so that messages from one transport (e.g. Zenoh)
/// are bridged onto another transport (e.g. SOME/IP).
///
/// # Examples
///
/// ## Typical usage
/// ```
/// use usubscription_static_file::USubscriptionStaticFile;
/// use std::sync::Arc;
/// use std::path::PathBuf;
/// use tokio::sync::Mutex;
/// use up_rust::{UListener, UTransport};
/// use up_streamer::{Endpoint, UStreamer};
/// # pub mod up_client_foo {
/// #     use std::sync::Arc;
/// use async_trait::async_trait;
/// #     use up_rust::{UListener, UMessage, UStatus, UUri};
/// #     use up_rust::UTransport;
/// #
/// #     pub struct UPClientFoo;
/// #
/// #     #[async_trait]
/// #     impl UTransport for UPClientFoo {
/// #         async fn send(&self, _message: UMessage) -> Result<(), UStatus> {
/// #             todo!()
/// #         }
/// #
/// #         async fn receive(
/// #             &self,
/// #            _source_filter: &UUri,
/// #            _sink_filter: Option<&UUri>,
/// #         ) -> Result<UMessage, UStatus> {
/// #             todo!()
/// #         }
/// #
/// #         async fn register_listener(
/// #                     &self,
/// #                     source_filter: &UUri,
/// #                     sink_filter: Option<&UUri>,
/// #                     listener: Arc<dyn UListener>,
/// #         ) -> Result<(), UStatus> {
/// #             println!("UPClientFoo: registering source_filter: {:?}", source_filter);
/// #             Ok(())
/// #         }
/// #
/// #         async fn unregister_listener(
/// #                     &self,
/// #                     source_filter: &UUri,
/// #                     sink_filter: Option<&UUri>,
/// #                     listener: Arc<dyn UListener>,
/// #         ) -> Result<(), UStatus> {
/// #             println!(
/// #                 "UPClientFoo: unregistering source_filter: {source_filter:?}"
/// #             );
/// #             Ok(())
/// #         }
/// #     }
/// #
/// #     impl UPClientFoo {
/// #         pub fn new() -> Self {
/// #             Self {}
/// #         }
/// #     }
/// # }
/// #
/// # pub mod up_client_bar {
/// #     use std::sync::Arc;
/// #     use async_trait::async_trait;
/// #     use up_rust::{UListener, UMessage, UStatus, UTransport, UUri};
/// #     pub struct UPClientBar;
/// #
/// #     #[async_trait]
/// #     impl UTransport for UPClientBar {
/// #         async fn send(&self, _message: UMessage) -> Result<(), UStatus> {
/// #             todo!()
/// #         }
/// #
/// #         async fn receive(
/// #             &self,
/// #            _source_filter: &UUri,
/// #            _sink_filter: Option<&UUri>,
/// #         ) -> Result<UMessage, UStatus> {
/// #             todo!()
/// #         }
/// #
/// #         async fn register_listener(
/// #                     &self,
/// #                     source_filter: &UUri,
/// #                     sink_filter: Option<&UUri>,
/// #                     listener: Arc<dyn UListener>,
/// #         ) -> Result<(), UStatus> {
/// #             println!("UPClientBar: registering source_filter: {:?}", source_filter);
/// #             Ok(())
/// #         }
/// #
/// #         async fn unregister_listener(
/// #                     &self,
/// #                     source_filter: &UUri,
/// #                     sink_filter: Option<&UUri>,
/// #                     listener: Arc<dyn UListener>,
/// #         ) -> Result<(), UStatus> {
/// #             println!(
/// #                 "UPClientBar: unregistering source_filter: {source_filter:?}"
/// #             );
/// #             Ok(())
/// #         }
/// #     }
/// #
/// #     impl UPClientBar {
/// #         pub fn new() -> Self {
/// #             Self {}
/// #         }
/// #     }
/// # }
/// #
/// # async fn async_main() {
///
/// // Local transport
/// let local_transport: Arc<dyn UTransport> = Arc::new(up_client_foo::UPClientFoo::new());
///
/// // Remote transport router
/// let remote_transport: Arc<dyn UTransport> = Arc::new(up_client_bar::UPClientBar::new());
///
/// // Local endpoint
/// let local_authority = "local";
/// let local_endpoint = Endpoint::new("local_endpoint", local_authority, local_transport);
///
/// // A remote endpoint
/// let remote_authority = "remote";
/// let remote_endpoint = Endpoint::new("remote_endpoint", remote_authority, remote_transport);
///
/// let subscription_path = "../utils/usubscription-static-file/static-configs/testdata.json".to_string();
/// let usubscription = Arc::new(USubscriptionStaticFile::new(subscription_path));
/// let mut streamer = match UStreamer::new(
///    "hoge",
///    100,
///    usubscription,
/// ) {
///     Ok(streamer) => streamer,
///     Err(error) => panic!("Failed to create uStreamer: {}", error),
/// };
///
/// // Add forwarding rules to endpoint local<->remote
/// assert_eq!(
///     streamer
///         .add_forwarding_rule(local_endpoint.clone(), remote_endpoint.clone())
///         .await,
///     Ok(())
/// );
/// assert_eq!(
///     streamer
///         .add_forwarding_rule(remote_endpoint.clone(), local_endpoint.clone())
///         .await,
///     Ok(())
/// );
///
/// // Add forwarding rules to endpoint local<->local, should report an error
/// assert!(streamer
///     .add_forwarding_rule(local_endpoint.clone(), local_endpoint.clone())
///     .await
///     .is_err());
///
/// // Rule already exists so it should report an error
/// assert!(streamer
///     .add_forwarding_rule(local_endpoint.clone(), remote_endpoint.clone())
///     .await
///     .is_err());
///
/// // Try and remove an invalid rule
/// assert!(streamer
///     .delete_forwarding_rule(remote_endpoint.clone(), remote_endpoint.clone())
///     .await
///     .is_err());
///
/// // remove valid routing rules
/// assert_eq!(
///     streamer
///         .delete_forwarding_rule(local_endpoint.clone(), remote_endpoint.clone())
///         .await,
///     Ok(())
/// );
/// assert_eq!(
///     streamer
///         .delete_forwarding_rule(remote_endpoint.clone(), local_endpoint.clone())
///         .await,
///     Ok(())
/// );
///
/// // Try and remove a rule that doesn't exist, should report an error
/// assert!(streamer
///     .delete_forwarding_rule(local_endpoint.clone(), remote_endpoint.clone())
///     .await
///     .is_err());
/// # }
/// ```
pub struct UStreamer {
    name: String,
    registered_forwarding_rules: ForwardingRules,
    transport_forwarders: TransportForwarders,
    forwarding_listeners: ForwardingListeners,
    subscription_cache: Arc<Mutex<SubscriptionCache>>,
}

impl UStreamer {
    /// Creates a new UStreamer which can be used to add forwarding rules.
    ///
    /// # Parameters
    ///
    /// * name - Used to uniquely identify this UStreamer in logs
    /// * message_queue_size - Determines size of channel used to communicate between `ForwardingListener`
    ///   and the worker tasks for each currently endpointd `UTransport`
    /// * usubscription - Subscription service which will be used to store subscription info for topics.
    pub fn new(
        name: &str,
        message_queue_size: u16,
        usubscription: Arc<dyn USubscription>,
    ) -> Result<Self, UStatus> {
        let name = format!("{USTREAMER_TAG}:{name}:");
        debug!(
            "{}:{}:{} UStreamer created",
            &name, USTREAMER_TAG, USTREAMER_FN_NEW_TAG
        );

        let uuri: UUri = UUri {
            authority_name: "*".to_string(),
            ue_id: 0x0000_FFFF,     // any instance, any service
            ue_version_major: 0xFF, // any
            resource_id: 0xFFFF,    // any
            ..Default::default()
        };

        let subscriber_info = SubscriberInfo {
            uri: Some(uuri.clone()).into(),
            ..Default::default()
        };

        // TODO: Create a NotificationsRequest and send over host transport

        // TODO: We need to form a FetchSubscriptionsRequest and send over host transport
        let mut fetch_request = FetchSubscriptionsRequest {
            request: None,
            ..Default::default()
        };
        fetch_request.set_subscriber(subscriber_info);
        let subscriptions = task::block_in_place(|| {
            CB_RUNTIME
                .block_on(usubscription.fetch_subscriptions(fetch_request))
                .expect("Failed to fetch subscriptions")
        });

        let subscription_cache_result = SubscriptionCache::new(subscriptions);

        let subscription_cache = match subscription_cache_result {
            Ok(cache) => {
                debug!(
                    "{}:{}:{} SubscriptionCache created",
                    name, USTREAMER_TAG, USTREAMER_FN_NEW_TAG
                );
                Arc::new(Mutex::new(cache))
            }
            Err(e) => {
                return Err(UStatus::fail_with_code(
                    UCode::INVALID_ARGUMENT,
                    format!(
                        "{}:{}:{} Unable to create SubscriptionCache: {:?}",
                        name, USTREAMER_TAG, USTREAMER_FN_NEW_TAG, e
                    ),
                ))
            }
        };

        Ok(Self {
            name: name.to_string(),
            registered_forwarding_rules: Mutex::new(HashSet::new()),
            transport_forwarders: TransportForwarders::new(message_queue_size as usize),
            forwarding_listeners: ForwardingListeners::new(),
            subscription_cache: subscription_cache.clone(),
        })
    }

    #[inline(always)]
    fn forwarding_id(r#in: &Endpoint, out: &Endpoint) -> String {
        format!(
            "[in.name: {}, in.authority: {:?} ; out.name: {}, out.authority: {:?}]",
            r#in.name, r#in.authority, out.name, out.authority
        )
    }

    #[inline(always)]
    fn fail_due_to_same_authority(&self, r#in: &Endpoint, out: &Endpoint) -> Result<(), UStatus> {
        let err = Err(UStatus::fail_with_code(
            UCode::INVALID_ARGUMENT,
            format!(
                "{} are the same. Unable to delete.",
                Self::forwarding_id(r#in, out)
            ),
        ));
        error!(
            "{}:{}:{} Deleting forwarding rule failed: {:?}",
            self.name, USTREAMER_TAG, USTREAMER_FN_ADD_FORWARDING_RULE_TAG, err
        );
        err
    }

    /// Adds a forwarding rule to the [`UStreamer`] based on an in [`Endpoint`][crate::Endpoint] and an
    /// out [`Endpoint`][crate::Endpoint]
    ///
    /// Works for any [`UMessage`][up_rust::UMessage] type which has a destination / sink contained
    /// in its attributes, i.e.
    /// * [`UMessageType::UMESSAGE_TYPE_NOTIFICATION`][up_rust::UMessageType::UMESSAGE_TYPE_NOTIFICATION]
    /// * [`UMessageType::UMESSAGE_TYPE_REQUEST`][up_rust::UMessageType::UMESSAGE_TYPE_REQUEST]
    /// * [`UMessageType::UMESSAGE_TYPE_RESPONSE`][up_rust::UMessageType::UMESSAGE_TYPE_RESPONSE]
    /// * [`UMessageType::UMESSAGE_TYPE_PUBLISH`][up_rust::UMessageType::UMESSAGE_TYPE_PUBLISH]
    ///
    /// # Parameters
    ///
    /// * `in` - [`Endpoint`][crate::Endpoint] we will bridge _from_
    /// * `out` - [`Endpoint`][crate::Endpoint] we will bridge _onto_
    ///
    /// # Errors
    ///
    /// If unable to add this forwarding rule, we return a [`UStatus`][up_rust::UStatus] noting
    /// the error.
    ///
    /// Typical errors include
    /// * already have this forwarding rule registered
    /// * attempting to forward onto the same [`Endpoint`][crate::Endpoint]
    pub async fn add_forwarding_rule(
        &mut self,
        r#in: Endpoint,
        out: Endpoint,
    ) -> Result<(), UStatus> {
        debug!(
            "{}:{}:{} Adding forwarding rule for {}",
            self.name,
            USTREAMER_TAG,
            USTREAMER_FN_ADD_FORWARDING_RULE_TAG,
            Self::forwarding_id(&r#in, &out)
        );

        if r#in.authority == out.authority {
            return self.fail_due_to_same_authority(&r#in, &out);
        }

        let in_comparable_transport = ComparableTransport::new(r#in.transport.clone());
        let out_comparable_transport = ComparableTransport::new(out.transport.clone());

        {
            let mut registered_forwarding_rules = self.registered_forwarding_rules.lock().await;
            match registered_forwarding_rules.insert((
                r#in.authority.clone(),
                out.authority.clone(),
                in_comparable_transport,
                out_comparable_transport,
            )) {
                true => {
                    let out_sender = self
                        .transport_forwarders
                        .insert(out.transport.clone())
                        .await;
                    if let Err(err) = self
                        .forwarding_listeners
                        .insert(
                            r#in.transport.clone(),
                            &r#in.authority,
                            &out.authority,
                            &Self::forwarding_id(&r#in, &out),
                            out_sender,
                            self.subscription_cache.clone(),
                        )
                        .await
                    {
                        return Err(UStatus::fail_with_code(
                            UCode::INVALID_ARGUMENT,
                            err.to_string(),
                        ));
                    };
                    Ok(())
                }
                false => {
                    // TODO: Add logging on inability to register
                    Err(UStatus::fail_with_code(
                        UCode::ALREADY_EXISTS,
                        "already exists",
                    ))
                }
            }
        }
    }

    /// Deletes a forwarding rule from the [`UStreamer`] based on an in [`Endpoint`][crate::Endpoint] and an
    /// out [`Endpoint`][crate::Endpoint]
    ///
    /// Works for any [`UMessage`][up_rust::UMessage] type which has a destination / sink contained
    /// in its attributes, i.e.
    /// * [`UMessageType::UMESSAGE_TYPE_NOTIFICATION`][up_rust::UMessageType::UMESSAGE_TYPE_NOTIFICATION]
    /// * [`UMessageType::UMESSAGE_TYPE_REQUEST`][up_rust::UMessageType::UMESSAGE_TYPE_REQUEST]
    /// * [`UMessageType::UMESSAGE_TYPE_RESPONSE`][up_rust::UMessageType::UMESSAGE_TYPE_RESPONSE]
    /// * [`UMessageType::UMESSAGE_TYPE_PUBLISH`][up_rust::UMessageType::UMESSAGE_TYPE_PUBLISH]
    ///
    /// # Parameters
    ///
    /// * `in` - [`Endpoint`][crate::Endpoint] we will bridge _from_
    /// * `out` - [`Endpoint`][crate::Endpoint] we will bridge _onto_
    ///
    /// # Errors
    ///
    /// If unable to delete this forwarding rule, we return a [`UStatus`][up_rust::UStatus] noting
    /// the error.
    ///
    /// Typical errors include
    /// * No such route has been added
    /// * attempting to delete a forwarding rule where we would forward onto the same [`Endpoint`][crate::Endpoint]
    pub async fn delete_forwarding_rule(
        &mut self,
        r#in: Endpoint,
        out: Endpoint,
    ) -> Result<(), UStatus> {
        debug!(
            "{}:{}:{} Deleting forwarding rule for {}",
            self.name,
            USTREAMER_TAG,
            USTREAMER_FN_DELETE_FORWARDING_RULE_TAG,
            Self::forwarding_id(&r#in, &out)
        );

        if r#in.authority == out.authority {
            return self.fail_due_to_same_authority(&r#in, &out);
        }

        let in_comparable_transport = ComparableTransport::new(r#in.transport.clone());
        let out_comparable_transport = ComparableTransport::new(out.transport.clone());

        let remove_res = {
            let mut registered_forwarding_rules = self.registered_forwarding_rules.lock().await;
            registered_forwarding_rules.remove(&(
                r#in.authority.clone(),
                out.authority.clone(),
                in_comparable_transport.clone(),
                out_comparable_transport.clone(),
            ))
        };

        match remove_res {
            true => {
                self.transport_forwarders
                    .remove(out.transport.clone())
                    .await;
                self.forwarding_listeners
                    .remove(
                        r#in.transport.clone(),
                        &r#in.authority,
                        &out.authority,
                        self.subscription_cache.clone(),
                    )
                    .await;
                Ok(())
            }
            false => Err(UStatus::fail_with_code(UCode::NOT_FOUND, "not found")),
        }
    }
}

#[derive(Clone)]
pub(crate) struct ComparableTransport {
    transport: Arc<dyn UTransport>,
}

impl ComparableTransport {
    pub fn new(transport: Arc<dyn UTransport>) -> Self {
        Self { transport }
    }
}

impl Hash for ComparableTransport {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.transport).hash(state);
    }
}

impl PartialEq for ComparableTransport {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.transport, &other.transport)
    }
}

impl Eq for ComparableTransport {}

const TRANSPORT_FORWARDER_TAG: &str = "TransportForwarder:";
const TRANSPORT_FORWARDER_FN_MESSAGE_FORWARDING_LOOP_TAG: &str = "message_forwarding_loop():";
pub(crate) struct TransportForwarder {}

impl TransportForwarder {
    fn new(out_transport: Arc<dyn UTransport>, message_receiver: Receiver<Arc<UMessage>>) -> Self {
        let out_transport_clone = out_transport.clone();
        let message_receiver_clone = message_receiver.resubscribe();

        thread::spawn(|| {
            // Create a new single-threaded runtime
            let runtime = Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create Tokio runtime");

            runtime.block_on(async move {
                trace!("Within blocked runtime");
                Self::message_forwarding_loop(
                    UUID::build().to_hyphenated_string(),
                    out_transport_clone,
                    message_receiver_clone,
                )
                .await;
                info!("Broke out of loop! You probably dropped the UPClientVsomeip");
            });
        });

        Self {}
    }

    async fn message_forwarding_loop(
        id: String,
        out_transport: Arc<dyn UTransport>,
        mut message_receiver: Receiver<Arc<UMessage>>,
    ) {
        while let Ok(msg) = message_receiver.recv().await {
            debug!(
                "{}:{}:{} Attempting send of message: {:?}",
                id,
                TRANSPORT_FORWARDER_TAG,
                TRANSPORT_FORWARDER_FN_MESSAGE_FORWARDING_LOOP_TAG,
                msg
            );
            let send_res = out_transport.send(msg.deref().clone()).await;
            if let Err(err) = send_res {
                warn!(
                    "{}:{}:{} Sending on out_transport failed: {:?}",
                    id,
                    TRANSPORT_FORWARDER_TAG,
                    TRANSPORT_FORWARDER_FN_MESSAGE_FORWARDING_LOOP_TAG,
                    err
                );
            } else {
                debug!(
                    "{}:{}:{} Sending on out_transport succeeded",
                    id, TRANSPORT_FORWARDER_TAG, TRANSPORT_FORWARDER_FN_MESSAGE_FORWARDING_LOOP_TAG
                );
            }
        }
    }
}

const FORWARDING_LISTENER_TAG: &str = "ForwardingListener:";
const FORWARDING_LISTENER_FN_ON_RECEIVE_TAG: &str = "on_receive():";

#[derive(Clone)]
pub(crate) struct ForwardingListener {
    forwarding_id: String,
    sender: Sender<Arc<UMessage>>,
}

impl ForwardingListener {
    pub(crate) fn new(forwarding_id: &str, sender: Sender<Arc<UMessage>>) -> Self {
        Self {
            forwarding_id: forwarding_id.to_string(),
            sender,
        }
    }
}

#[async_trait]
impl UListener for ForwardingListener {
    async fn on_receive(&self, msg: UMessage) {
        debug!(
            "{}:{}:{} Received message: {:?}",
            self.forwarding_id,
            FORWARDING_LISTENER_TAG,
            FORWARDING_LISTENER_FN_ON_RECEIVE_TAG,
            &msg
        );

        if msg.attributes.payload_format.enum_value_or_default()
            == UPayloadFormat::UPAYLOAD_FORMAT_SHM
        {
            debug!(
                "{}:{}:{} Received message with type UPAYLOAD_FORMAT_SHM, \
                which is not supported. A pointer to shared memory will not \
                be usable on another device. UAttributes: {:#?}",
                self.forwarding_id,
                FORWARDING_LISTENER_TAG,
                FORWARDING_LISTENER_FN_ON_RECEIVE_TAG,
                &msg.attributes
            );
            return;
        }
        if let Err(e) = self.sender.send(Arc::new(msg)) {
            error!(
                "{}:{}:{} Unable to send message to worker pool: {e:?}",
                self.forwarding_id, FORWARDING_LISTENER_TAG, FORWARDING_LISTENER_FN_ON_RECEIVE_TAG,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ustreamer::{uauthority_to_uuri, ForwardingListeners};
    use crate::{Endpoint, UStreamer};
    use async_trait::async_trait;
    use std::str::FromStr;
    use std::sync::{Arc, Mutex as StdMutex};
    use subscription_cache::SubscriptionCache;
    use tokio::sync::Mutex as TokioMutex;
    use up_rust::core::usubscription::{FetchSubscriptionsResponse, SubscriberInfo, Subscription};
    use up_rust::{UCode, UListener, UMessage, UStatus, UTransport, UUri};
    use usubscription_static_file::USubscriptionStaticFile;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct ListenerRegistration {
        source_filter: UUri,
        sink_filter: Option<UUri>,
    }

    #[derive(Default)]
    struct RecordingTransport {
        register_calls: StdMutex<Vec<ListenerRegistration>>,
        unregister_calls: StdMutex<Vec<ListenerRegistration>>,
    }

    impl RecordingTransport {
        fn register_calls(&self) -> Vec<ListenerRegistration> {
            self.register_calls.lock().unwrap().clone()
        }

        fn unregister_calls(&self) -> Vec<ListenerRegistration> {
            self.unregister_calls.lock().unwrap().clone()
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
                "not implemented",
            ))
        }

        async fn register_listener(
            &self,
            source_filter: &UUri,
            sink_filter: Option<&UUri>,
            _listener: Arc<dyn UListener>,
        ) -> Result<(), UStatus> {
            self.register_calls
                .lock()
                .unwrap()
                .push(ListenerRegistration {
                    source_filter: source_filter.clone(),
                    sink_filter: sink_filter.cloned(),
                });
            Ok(())
        }

        async fn unregister_listener(
            &self,
            source_filter: &UUri,
            sink_filter: Option<&UUri>,
            _listener: Arc<dyn UListener>,
        ) -> Result<(), UStatus> {
            self.unregister_calls
                .lock()
                .unwrap()
                .push(ListenerRegistration {
                    source_filter: source_filter.clone(),
                    sink_filter: sink_filter.cloned(),
                });
            Ok(())
        }
    }

    fn make_subscription_cache(entries: &[(&str, &str)]) -> Arc<TokioMutex<SubscriptionCache>> {
        let subscriptions = entries
            .iter()
            .map(|(topic, subscriber)| {
                let topic_uri = UUri::from_str(topic).unwrap();
                let subscriber_uri = UUri::from_str(subscriber).unwrap();
                let subscriber_info = SubscriberInfo {
                    uri: Some(subscriber_uri).into(),
                    ..Default::default()
                };

                Subscription {
                    topic: Some(topic_uri).into(),
                    subscriber: Some(subscriber_info).into(),
                    ..Default::default()
                }
            })
            .collect();

        Arc::new(TokioMutex::new(
            SubscriptionCache::new(FetchSubscriptionsResponse {
                subscriptions,
                ..Default::default()
            })
            .unwrap(),
        ))
    }

    fn has_listener_call(
        calls: &[ListenerRegistration],
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
    ) -> bool {
        calls.iter().any(|call| {
            call.source_filter == *source_filter && call.sink_filter.as_ref() == sink_filter
        })
    }

    pub struct UPClientFoo;

    #[async_trait]
    impl UTransport for UPClientFoo {
        async fn send(&self, _message: UMessage) -> Result<(), UStatus> {
            todo!()
        }

        async fn receive(
            &self,
            _source_filter: &UUri,
            _sink_filter: Option<&UUri>,
        ) -> Result<UMessage, UStatus> {
            todo!()
        }

        async fn register_listener(
            &self,
            source_filter: &UUri,
            _sink_filter: Option<&UUri>,
            _listener: Arc<dyn UListener>,
        ) -> Result<(), UStatus> {
            println!(
                "UPClientFoo: registering source_filter: {:?}",
                source_filter
            );
            Ok(())
        }

        async fn unregister_listener(
            &self,
            source_filter: &UUri,
            _sink_filter: Option<&UUri>,
            _listener: Arc<dyn UListener>,
        ) -> Result<(), UStatus> {
            println!(
                "UPClientFoo: unregistering source_filter: {:?}",
                source_filter
            );
            Ok(())
        }
    }

    pub struct UPClientBar;

    #[async_trait]
    impl UTransport for UPClientBar {
        async fn send(&self, _message: UMessage) -> Result<(), UStatus> {
            todo!()
        }

        async fn receive(
            &self,
            _source_filter: &UUri,
            _sink_filter: Option<&UUri>,
        ) -> Result<UMessage, UStatus> {
            todo!()
        }

        async fn register_listener(
            &self,
            source_filter: &UUri,
            _sink_filter: Option<&UUri>,
            _listener: Arc<dyn UListener>,
        ) -> Result<(), UStatus> {
            println!(
                "UPClientBar: registering source_filter: {:?}",
                source_filter
            );
            Ok(())
        }

        async fn unregister_listener(
            &self,
            source_filter: &UUri,
            _sink_filter: Option<&UUri>,
            _listener: Arc<dyn UListener>,
        ) -> Result<(), UStatus> {
            println!(
                "UPClientBar: unregistering source_filter: {:?}",
                source_filter
            );
            Ok(())
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_simple_with_a_single_input_and_output_endpoint() {
        // Local endpoint
        let local_authority = "local";
        let local_transport: Arc<dyn UTransport> = Arc::new(UPClientFoo);
        let local_endpoint =
            Endpoint::new("local_endpoint", local_authority, local_transport.clone());

        // A remote endpoint
        let remote_authority = "remote";
        let remote_transport: Arc<dyn UTransport> = Arc::new(UPClientBar);
        let remote_endpoint = Endpoint::new(
            "remote_endpoint",
            remote_authority,
            remote_transport.clone(),
        );

        let subscription_path =
            "../utils/usubscription-static-file/static-configs/testdata.json".to_string();
        let usubscription = Arc::new(USubscriptionStaticFile::new(subscription_path));
        let mut ustreamer = match UStreamer::new("foo_bar_streamer", 100, usubscription) {
            Ok(streamer) => streamer,
            Err(error) => panic!("Failed to create uStreamer: {}", error),
        };
        // Add forwarding rules to endpoint local<->remote
        assert!(ustreamer
            .add_forwarding_rule(local_endpoint.clone(), remote_endpoint.clone())
            .await
            .is_ok());
        assert!(ustreamer
            .add_forwarding_rule(remote_endpoint.clone(), local_endpoint.clone())
            .await
            .is_ok());

        // Add forwarding rules to endpoint local<->local, should report an error
        assert!(ustreamer
            .add_forwarding_rule(local_endpoint.clone(), local_endpoint.clone())
            .await
            .is_err());

        // Rule already exists so it should report an error
        assert!(ustreamer
            .add_forwarding_rule(local_endpoint.clone(), remote_endpoint.clone())
            .await
            .is_err());

        // Add forwarding rules to endpoint remote<->remote, should report an error
        assert!(ustreamer
            .add_forwarding_rule(remote_endpoint.clone(), remote_endpoint.clone())
            .await
            .is_err());

        // remove valid routing rules
        assert!(ustreamer
            .delete_forwarding_rule(local_endpoint.clone(), remote_endpoint.clone())
            .await
            .is_ok());
        assert!(ustreamer
            .delete_forwarding_rule(remote_endpoint.clone(), local_endpoint.clone())
            .await
            .is_ok());

        // Try and remove a rule that doesn't exist, should report an error
        assert!(ustreamer
            .delete_forwarding_rule(local_endpoint.clone(), remote_endpoint.clone())
            .await
            .is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_advanced_where_there_is_a_local_endpoint_and_two_remote_endpoints() {
        // Local endpoint
        let local_authority = "local";
        let local_transport: Arc<dyn UTransport> = Arc::new(UPClientFoo);
        let local_endpoint =
            Endpoint::new("local_endpoint", local_authority, local_transport.clone());

        // Remote endpoint - A
        let remote_authority_a = "remote_a";
        let remote_transport_a: Arc<dyn UTransport> = Arc::new(UPClientBar);
        let remote_endpoint_a = Endpoint::new(
            "remote_endpoint_a",
            remote_authority_a,
            remote_transport_a.clone(),
        );

        // Remote endpoint - B
        let remote_authority_b = "remote_b";
        let remote_transport_b: Arc<dyn UTransport> = Arc::new(UPClientBar);
        let remote_endpoint_b = Endpoint::new(
            "remote_endpoint_b",
            remote_authority_b,
            remote_transport_b.clone(),
        );

        let subscription_path =
            "../utils/usubscription-static-file/static-configs/testdata.json".to_string();
        let usubscription = Arc::new(USubscriptionStaticFile::new(subscription_path));
        let mut ustreamer = match UStreamer::new("foo_bar_streamer", 100, usubscription) {
            Ok(streamer) => streamer,
            Err(error) => panic!("Failed to create uStreamer: {}", error),
        };

        // Add forwarding rules to endpoint local<->remote_a
        assert!(ustreamer
            .add_forwarding_rule(local_endpoint.clone(), remote_endpoint_a.clone())
            .await
            .is_ok());
        assert!(ustreamer
            .add_forwarding_rule(remote_endpoint_a.clone(), local_endpoint.clone())
            .await
            .is_ok());

        // Add forwarding rules to endpoint local<->remote_b
        assert!(ustreamer
            .add_forwarding_rule(local_endpoint.clone(), remote_endpoint_b.clone())
            .await
            .is_ok());
        assert!(ustreamer
            .add_forwarding_rule(remote_endpoint_b.clone(), local_endpoint.clone())
            .await
            .is_ok());

        // Add forwarding rules to endpoint remote_a<->remote_b
        assert!(ustreamer
            .add_forwarding_rule(remote_endpoint_a.clone(), remote_endpoint_b.clone())
            .await
            .is_ok());
        assert!(ustreamer
            .add_forwarding_rule(remote_endpoint_b.clone(), remote_endpoint_a.clone())
            .await
            .is_ok());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_advanced_where_there_is_a_local_endpoint_and_two_remote_endpoints_but_the_remote_endpoints_have_the_same_instance_of_utransport(
    ) {
        // Local endpoint
        let local_authority = "local";
        let local_transport: Arc<dyn UTransport> = Arc::new(UPClientFoo);
        let local_endpoint =
            Endpoint::new("local_endpoint", local_authority, local_transport.clone());

        let remote_transport: Arc<dyn UTransport> = Arc::new(UPClientBar);

        // Remote endpoint - A
        let remote_authority_a = "remote_a";
        let remote_endpoint_a = Endpoint::new(
            "remote_endpoint_a",
            remote_authority_a,
            remote_transport.clone(),
        );

        // Remote endpoint - B
        let remote_authority_b = "remote_b";
        let remote_endpoint_b = Endpoint::new(
            "remote_endpoint_b",
            remote_authority_b,
            remote_transport.clone(),
        );

        let subscription_path =
            "../utils/usubscription-static-file/static-configs/testdata.json".to_string();
        let usubscription = Arc::new(USubscriptionStaticFile::new(subscription_path));
        let mut ustreamer = match UStreamer::new("foo_bar_streamer", 100, usubscription) {
            Ok(streamer) => streamer,
            Err(error) => panic!("Failed to create uStreamer: {}", error),
        };

        // Add forwarding rules to endpoint local<->remote_a
        assert!(ustreamer
            .add_forwarding_rule(local_endpoint.clone(), remote_endpoint_a.clone())
            .await
            .is_ok());
        assert!(ustreamer
            .add_forwarding_rule(remote_endpoint_a.clone(), local_endpoint.clone())
            .await
            .is_ok());

        // Add forwarding rules to endpoint local<->remote_b
        assert!(ustreamer
            .add_forwarding_rule(local_endpoint.clone(), remote_endpoint_b.clone())
            .await
            .is_ok());
        assert!(ustreamer
            .add_forwarding_rule(remote_endpoint_b.clone(), local_endpoint.clone())
            .await
            .is_ok());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn issue_74_publish_filter_respects_topic_authority() {
        let recording_transport = Arc::new(RecordingTransport::default());
        let in_transport: Arc<dyn UTransport> = recording_transport.clone();
        let forwarding_listeners = ForwardingListeners::new();
        let (out_sender, _) = tokio::sync::broadcast::channel(16);
        let subscription_cache =
            make_subscription_cache(&[("//authority-a/5BA0/1/8001", "//authority-b/5678/1/1234")]);

        let insert_result = forwarding_listeners
            .insert(
                in_transport,
                "authority-a",
                "authority-b",
                "issue-74",
                out_sender,
                subscription_cache,
            )
            .await;

        assert!(insert_result.is_ok());

        let register_calls = recording_transport.register_calls();
        let request_source = uauthority_to_uuri("authority-a");
        let request_sink = uauthority_to_uuri("authority-b");
        let publish_source = UUri::try_from_parts("authority-a", 0x5BA0, 0x1, 0x8001).unwrap();

        assert!(has_listener_call(
            &register_calls,
            &request_source,
            Some(&request_sink)
        ));
        assert!(has_listener_call(&register_calls, &publish_source, None));
        assert_eq!(
            register_calls
                .iter()
                .filter(|call| call.sink_filter.is_none())
                .count(),
            1
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn issue_74_publish_filter_allows_left_wildcard() {
        let recording_transport = Arc::new(RecordingTransport::default());
        let in_transport: Arc<dyn UTransport> = recording_transport.clone();
        let forwarding_listeners = ForwardingListeners::new();
        let (out_sender, _) = tokio::sync::broadcast::channel(16);
        let subscription_cache =
            make_subscription_cache(&[("//*/5BA0/1/8001", "//authority-b/5678/1/1234")]);

        let insert_result = forwarding_listeners
            .insert(
                in_transport,
                "authority-c",
                "authority-b",
                "issue-74",
                out_sender,
                subscription_cache,
            )
            .await;

        assert!(insert_result.is_ok());

        let register_calls = recording_transport.register_calls();
        let publish_source = UUri::try_from_parts("authority-c", 0x5BA0, 0x1, 0x8001).unwrap();

        assert!(has_listener_call(&register_calls, &publish_source, None));
        assert_eq!(
            register_calls
                .iter()
                .filter(|call| call.sink_filter.is_none())
                .count(),
            1
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn issue_74_publish_filter_blocks_unmapped_authority() {
        let recording_transport = Arc::new(RecordingTransport::default());
        let in_transport: Arc<dyn UTransport> = recording_transport.clone();
        let forwarding_listeners = ForwardingListeners::new();
        let (out_sender, _) = tokio::sync::broadcast::channel(16);
        let subscription_cache =
            make_subscription_cache(&[("//authority-a/5BA0/1/8001", "//authority-b/5678/1/1234")]);

        let insert_result = forwarding_listeners
            .insert(
                in_transport,
                "authority-c",
                "authority-b",
                "issue-74",
                out_sender,
                subscription_cache,
            )
            .await;

        assert!(insert_result.is_ok());

        let register_calls = recording_transport.register_calls();
        let request_source = uauthority_to_uuri("authority-c");
        let request_sink = uauthority_to_uuri("authority-b");
        let disallowed_publish_source =
            UUri::try_from_parts("authority-c", 0x5BA0, 0x1, 0x8001).unwrap();

        assert!(has_listener_call(
            &register_calls,
            &request_source,
            Some(&request_sink)
        ));
        assert!(!has_listener_call(
            &register_calls,
            &disallowed_publish_source,
            None
        ));
        assert_eq!(
            register_calls
                .iter()
                .filter(|call| call.sink_filter.is_none())
                .count(),
            0
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn issue_74_unregistration_removes_publish_filters() {
        let recording_transport = Arc::new(RecordingTransport::default());
        let in_transport: Arc<dyn UTransport> = recording_transport.clone();
        let forwarding_listeners = ForwardingListeners::new();
        let (out_sender, _) = tokio::sync::broadcast::channel(16);
        let subscription_cache =
            make_subscription_cache(&[("//authority-a/5BA0/1/8001", "//authority-b/5678/1/1234")]);

        let insert_result = forwarding_listeners
            .insert(
                in_transport.clone(),
                "authority-a",
                "authority-b",
                "issue-74",
                out_sender,
                subscription_cache.clone(),
            )
            .await;

        assert!(insert_result.is_ok());

        forwarding_listeners
            .remove(
                in_transport,
                "authority-a",
                "authority-b",
                subscription_cache,
            )
            .await;

        let unregister_calls = recording_transport.unregister_calls();
        let request_source = uauthority_to_uuri("authority-a");
        let request_sink = uauthority_to_uuri("authority-b");
        let publish_source = UUri::try_from_parts("authority-a", 0x5BA0, 0x1, 0x8001).unwrap();

        assert!(has_listener_call(
            &unregister_calls,
            &request_source,
            Some(&request_sink)
        ));
        assert!(has_listener_call(&unregister_calls, &publish_source, None));
    }
}

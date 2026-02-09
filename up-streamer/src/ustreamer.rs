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

use crate::control_plane::route_lifecycle::{AddRouteError, RemoveRouteError, RouteLifecycle};
use crate::control_plane::route_table::RouteTable;
use crate::data_plane::egress_pool::EgressRoutePool;
use crate::data_plane::ingress_registry::IngressRouteRegistry;
use crate::endpoint::Endpoint;
use crate::routing::subscription_directory::SubscriptionDirectory;
use crate::runtime::subscription_runtime::fetch_subscriptions;
use std::sync::Arc;
use subscription_cache::SubscriptionCache;
use tokio::sync::Mutex;
use tracing::{debug, error};
use up_rust::core::usubscription::{FetchSubscriptionsRequest, SubscriberInfo, USubscription};
use up_rust::{UCode, UStatus, UUri};

const USTREAMER_TAG: &str = "UStreamer:";
const USTREAMER_FN_NEW_TAG: &str = "new():";
const USTREAMER_FN_ADD_ROUTE_TAG: &str = "add_route():";
const USTREAMER_FN_DELETE_ROUTE_TAG: &str = "delete_route():";

pub struct UStreamer {
    name: String,
    route_table: RouteTable,
    egress_route_pool: EgressRoutePool,
    ingress_route_registry: IngressRouteRegistry,
    subscription_directory: SubscriptionDirectory,
}

impl UStreamer {
    /// Creates a streamer instance with preloaded subscription directory state.
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
            ue_id: 0x0000_FFFF,
            ue_version_major: 0xFF,
            resource_id: 0xFFFF,
            ..Default::default()
        };

        let subscriber_info = SubscriberInfo {
            uri: Some(uuri).into(),
            ..Default::default()
        };

        let mut fetch_request = FetchSubscriptionsRequest {
            request: None,
            ..Default::default()
        };
        fetch_request.set_subscriber(subscriber_info);

        let subscriptions = fetch_subscriptions(usubscription, fetch_request);
        let subscription_directory = match SubscriptionCache::new(subscriptions) {
            Ok(cache) => SubscriptionDirectory::new(Arc::new(Mutex::new(cache))),
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
            route_table: RouteTable::new(),
            egress_route_pool: EgressRoutePool::new(message_queue_size as usize),
            ingress_route_registry: IngressRouteRegistry::new(),
            subscription_directory,
        })
    }

    #[inline(always)]
    fn route_label(r#in: &Endpoint, out: &Endpoint) -> String {
        format!(
            "[in.name: {}, in.authority: {:?} ; out.name: {}, out.authority: {:?}]",
            r#in.name, r#in.authority, out.name, out.authority
        )
    }

    #[inline(always)]
    fn fail_due_to_same_authority(
        &self,
        action_tag: &str,
        r#in: &Endpoint,
        out: &Endpoint,
    ) -> Result<(), UStatus> {
        let err = Err(UStatus::fail_with_code(
            UCode::INVALID_ARGUMENT,
            format!(
                "{} are the same. Unable to delete.",
                Self::route_label(r#in, out)
            ),
        ));
        error!(
            "{}:{}:{} route operation failed: {:?}",
            self.name, USTREAMER_TAG, action_tag, err
        );
        err
    }

    /// Adds a unidirectional route between ingress and egress endpoints.
    pub async fn add_route(&mut self, r#in: Endpoint, out: Endpoint) -> Result<(), UStatus> {
        let route_label = Self::route_label(&r#in, &out);
        debug!(
            "{}:{}:{} Adding route for {}",
            self.name, USTREAMER_TAG, USTREAMER_FN_ADD_ROUTE_TAG, route_label
        );

        let mut lifecycle = RouteLifecycle::new(
            &self.route_table,
            &mut self.egress_route_pool,
            &self.ingress_route_registry,
            &self.subscription_directory,
        );

        match lifecycle.add_route(&r#in, &out, &route_label).await {
            Ok(()) => Ok(()),
            Err(AddRouteError::SameAuthority) => {
                self.fail_due_to_same_authority(USTREAMER_FN_ADD_ROUTE_TAG, &r#in, &out)
            }
            Err(AddRouteError::AlreadyExists) => Err(UStatus::fail_with_code(
                UCode::ALREADY_EXISTS,
                "already exists",
            )),
            Err(AddRouteError::FailedToRegisterIngressRoute(err)) => Err(UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                err.to_string(),
            )),
        }
    }

    /// Deletes a previously registered unidirectional route.
    pub async fn delete_route(&mut self, r#in: Endpoint, out: Endpoint) -> Result<(), UStatus> {
        let route_label = Self::route_label(&r#in, &out);
        debug!(
            "{}:{}:{} Deleting route for {}",
            self.name, USTREAMER_TAG, USTREAMER_FN_DELETE_ROUTE_TAG, route_label
        );

        let mut lifecycle = RouteLifecycle::new(
            &self.route_table,
            &mut self.egress_route_pool,
            &self.ingress_route_registry,
            &self.subscription_directory,
        );

        match lifecycle.remove_route(&r#in, &out).await {
            Ok(()) => Ok(()),
            Err(RemoveRouteError::SameAuthority) => {
                self.fail_due_to_same_authority(USTREAMER_FN_DELETE_ROUTE_TAG, &r#in, &out)
            }
            Err(RemoveRouteError::NotFound) => {
                Err(UStatus::fail_with_code(UCode::NOT_FOUND, "not found"))
            }
        }
    }

    /// Backward-compatible API alias for [`UStreamer::add_route`].
    pub async fn add_forwarding_rule(
        &mut self,
        r#in: Endpoint,
        out: Endpoint,
    ) -> Result<(), UStatus> {
        self.add_route(r#in, out).await
    }

    /// Backward-compatible API alias for [`UStreamer::delete_route`].
    pub async fn delete_forwarding_rule(
        &mut self,
        r#in: Endpoint,
        out: Endpoint,
    ) -> Result<(), UStatus> {
        self.delete_route(r#in, out).await
    }
}

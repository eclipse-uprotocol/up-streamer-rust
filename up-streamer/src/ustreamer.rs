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
use crate::observability::events;
use crate::routing::subscription_directory::SubscriptionDirectory;
use crate::subscription_sync_health::SubscriptionSyncHealth;
use std::sync::Arc;
use tracing::{debug, error, warn};
use up_rust::core::usubscription::{
    FetchSubscriptionsRequest, FetchSubscriptionsResponse, USubscription,
};
use up_rust::{UCode, UStatus};

const COMPONENT: &str = "ustreamer";

pub struct UStreamer {
    name: String,
    route_table: RouteTable,
    egress_route_pool: EgressRoutePool,
    ingress_route_registry: IngressRouteRegistry,
    subscription_directory: SubscriptionDirectory,
    usubscription: Arc<dyn USubscription>,
    subscription_sync_health: SubscriptionSyncHealth,
}

impl UStreamer {
    /// Creates a streamer instance with preloaded subscription directory state.
    pub async fn new(
        name: &str,
        message_queue_size: u16,
        usubscription: Arc<dyn USubscription>,
    ) -> Result<Self, UStatus> {
        let name = name.to_string();
        debug!(
            event = "ustreamer_create",
            component = COMPONENT,
            streamer_name = name.as_str(),
            "UStreamer created"
        );

        let mut streamer = Self {
            name,
            route_table: RouteTable::new(),
            egress_route_pool: EgressRoutePool::new(message_queue_size as usize),
            ingress_route_registry: IngressRouteRegistry::new(),
            subscription_directory: SubscriptionDirectory::empty(),
            usubscription,
            subscription_sync_health: SubscriptionSyncHealth::default(),
        };

        if let Err(err) = streamer.refresh_subscriptions().await {
            warn!(
                event = "subscription_bootstrap_failed",
                component = COMPONENT,
                streamer_name = streamer.name.as_str(),
                err = %err,
                "startup subscription bootstrap failed; deferred-refresh mode active"
            );
        }

        Ok(streamer)
    }

    fn update_subscription_sync_health(&mut self, succeeded: bool) -> SubscriptionSyncHealth {
        let now = std::time::SystemTime::now();
        self.subscription_sync_health.previous_attempt_succeeded =
            self.subscription_sync_health.last_attempt_succeeded;
        self.subscription_sync_health.last_attempt_at = Some(now);
        self.subscription_sync_health.last_attempt_succeeded = Some(succeeded);
        if succeeded {
            self.subscription_sync_health.last_success_at = Some(now);
        }
        self.subscription_sync_health.clone()
    }

    async fn apply_subscription_snapshot(
        &mut self,
        snapshot: FetchSubscriptionsResponse,
    ) -> Result<(), UStatus> {
        self.subscription_directory.apply_snapshot(snapshot).await
    }

    pub async fn refresh_subscriptions(&mut self) -> Result<SubscriptionSyncHealth, UStatus> {
        let snapshot = match self
            .usubscription
            .fetch_subscriptions(FetchSubscriptionsRequest::default())
            .await
        {
            Ok(snapshot) => snapshot,
            Err(err) => {
                self.update_subscription_sync_health(false);
                return Err(err);
            }
        };

        if let Err(err) = self.apply_subscription_snapshot(snapshot).await {
            self.update_subscription_sync_health(false);
            return Err(err);
        }

        Ok(self.update_subscription_sync_health(true))
    }

    pub fn subscription_sync_health(&self) -> SubscriptionSyncHealth {
        self.subscription_sync_health.clone()
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
        failure_event: &str,
        route_label: &str,
        r#in: &Endpoint,
        out: &Endpoint,
        action: &str,
    ) -> Result<(), UStatus> {
        let err = Err(UStatus::fail_with_code(
            UCode::INVALID_ARGUMENT,
            format!(
                "{} are the same. Unable to {}.",
                Self::route_label(r#in, out),
                action,
            ),
        ));
        error!(
            event = failure_event,
            component = COMPONENT,
            streamer_name = self.name.as_str(),
            route_label,
            in_authority = r#in.authority.as_str(),
            out_authority = out.authority.as_str(),
            reason = "same_authority",
            err = ?err,
            "route operation failed"
        );
        err
    }

    /// Adds a unidirectional route between ingress and egress endpoints.
    pub async fn add_route_ref(
        &mut self,
        in_ep: &Endpoint,
        out_ep: &Endpoint,
    ) -> Result<(), UStatus> {
        let route_label = Self::route_label(in_ep, out_ep);
        debug!(
            event = events::ROUTE_ADD_START,
            component = COMPONENT,
            streamer_name = self.name.as_str(),
            route_label,
            in_authority = in_ep.authority.as_str(),
            out_authority = out_ep.authority.as_str(),
            "adding route"
        );

        let lifecycle = RouteLifecycle::new(
            &self.route_table,
            &self.ingress_route_registry,
            &self.subscription_directory,
        );

        match lifecycle
            .add_route(&mut self.egress_route_pool, in_ep, out_ep, &route_label)
            .await
        {
            Ok(()) => {
                debug!(
                    event = events::ROUTE_ADD_OK,
                    component = COMPONENT,
                    streamer_name = self.name.as_str(),
                    route_label,
                    in_authority = in_ep.authority.as_str(),
                    out_authority = out_ep.authority.as_str(),
                    "route add succeeded"
                );
                Ok(())
            }
            Err(AddRouteError::SameAuthority) => self.fail_due_to_same_authority(
                events::ROUTE_ADD_FAILED,
                &route_label,
                in_ep,
                out_ep,
                "add",
            ),
            Err(AddRouteError::AlreadyExists) => {
                error!(
                    event = events::ROUTE_ADD_FAILED,
                    component = COMPONENT,
                    streamer_name = self.name.as_str(),
                    route_label,
                    in_authority = in_ep.authority.as_str(),
                    out_authority = out_ep.authority.as_str(),
                    reason = "already_exists",
                    "route add failed because route already exists"
                );
                Err(UStatus::fail_with_code(
                    UCode::ALREADY_EXISTS,
                    "already exists",
                ))
            }
            Err(AddRouteError::FailedToRegisterIngressRoute(err)) => {
                error!(
                    event = events::ROUTE_ADD_FAILED,
                    component = COMPONENT,
                    streamer_name = self.name.as_str(),
                    route_label,
                    in_authority = in_ep.authority.as_str(),
                    out_authority = out_ep.authority.as_str(),
                    reason = "ingress_registration_failed",
                    err = %err,
                    "route add failed during ingress registration"
                );
                Err(UStatus::fail_with_code(
                    UCode::INVALID_ARGUMENT,
                    err.to_string(),
                ))
            }
        }
    }

    /// Adds a unidirectional route between ingress and egress endpoints.
    pub async fn add_route(&mut self, r#in: Endpoint, out: Endpoint) -> Result<(), UStatus> {
        self.add_route_ref(&r#in, &out).await
    }

    /// Deletes a previously registered unidirectional route.
    pub async fn delete_route_ref(
        &mut self,
        in_ep: &Endpoint,
        out_ep: &Endpoint,
    ) -> Result<(), UStatus> {
        let route_label = Self::route_label(in_ep, out_ep);
        debug!(
            event = events::ROUTE_DELETE_START,
            component = COMPONENT,
            streamer_name = self.name.as_str(),
            route_label,
            in_authority = in_ep.authority.as_str(),
            out_authority = out_ep.authority.as_str(),
            "deleting route"
        );

        let lifecycle = RouteLifecycle::new(
            &self.route_table,
            &self.ingress_route_registry,
            &self.subscription_directory,
        );

        match lifecycle
            .remove_route(&mut self.egress_route_pool, in_ep, out_ep, &route_label)
            .await
        {
            Ok(()) => {
                debug!(
                    event = events::ROUTE_DELETE_OK,
                    component = COMPONENT,
                    streamer_name = self.name.as_str(),
                    route_label,
                    in_authority = in_ep.authority.as_str(),
                    out_authority = out_ep.authority.as_str(),
                    "route delete succeeded"
                );
                Ok(())
            }
            Err(RemoveRouteError::SameAuthority) => self.fail_due_to_same_authority(
                events::ROUTE_DELETE_FAILED,
                &route_label,
                in_ep,
                out_ep,
                "delete",
            ),
            Err(RemoveRouteError::NotFound) => {
                error!(
                    event = events::ROUTE_DELETE_FAILED,
                    component = COMPONENT,
                    streamer_name = self.name.as_str(),
                    route_label,
                    in_authority = in_ep.authority.as_str(),
                    out_authority = out_ep.authority.as_str(),
                    reason = "not_found",
                    "route delete failed because route was not found"
                );
                Err(UStatus::fail_with_code(UCode::NOT_FOUND, "not found"))
            }
        }
    }

    /// Deletes a previously registered unidirectional route.
    pub async fn delete_route(&mut self, r#in: Endpoint, out: Endpoint) -> Result<(), UStatus> {
        self.delete_route_ref(&r#in, &out).await
    }
}

#[cfg(test)]
mod tests {
    use super::UStreamer;
    use crate::SubscriptionSyncHealth;
    use async_trait::async_trait;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use up_rust::core::usubscription::{
        FetchSubscribersRequest, FetchSubscribersResponse, FetchSubscriptionsRequest,
        FetchSubscriptionsResponse, NotificationsRequest, ResetRequest, ResetResponse,
        SubscriberInfo, Subscription, SubscriptionRequest, SubscriptionResponse, USubscription,
        UnsubscribeRequest,
    };
    use up_rust::{UCode, UStatus, UUri};

    struct SequencedUSubscription {
        responses: Mutex<VecDeque<Result<FetchSubscriptionsResponse, UStatus>>>,
    }

    impl SequencedUSubscription {
        fn new(responses: Vec<Result<FetchSubscriptionsResponse, UStatus>>) -> Self {
            Self {
                responses: Mutex::new(VecDeque::from(responses)),
            }
        }

        fn unsupported(operation: &str) -> UStatus {
            UStatus::fail_with_code(
                UCode::UNIMPLEMENTED,
                format!("{operation} is not used in this test stub"),
            )
        }
    }

    #[async_trait]
    impl USubscription for SequencedUSubscription {
        async fn subscribe(
            &self,
            _subscription_request: SubscriptionRequest,
        ) -> Result<SubscriptionResponse, UStatus> {
            Err(Self::unsupported("subscribe"))
        }

        async fn fetch_subscriptions(
            &self,
            _fetch_subscriptions_request: FetchSubscriptionsRequest,
        ) -> Result<FetchSubscriptionsResponse, UStatus> {
            self.responses
                .lock()
                .expect("provider queue lock should succeed")
                .pop_front()
                .unwrap_or_else(|| {
                    Err(UStatus::fail_with_code(
                        UCode::UNAVAILABLE,
                        "no queued snapshot response",
                    ))
                })
        }

        async fn unsubscribe(
            &self,
            _unsubscribe_request: UnsubscribeRequest,
        ) -> Result<(), UStatus> {
            Err(Self::unsupported("unsubscribe"))
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
            _fetch_subscribers_request: FetchSubscribersRequest,
        ) -> Result<FetchSubscribersResponse, UStatus> {
            Err(Self::unsupported("fetch_subscribers"))
        }

        async fn reset(&self, _reset_request: ResetRequest) -> Result<ResetResponse, UStatus> {
            Ok(ResetResponse::default())
        }
    }

    fn subscription(topic: &str, subscriber: &str) -> Subscription {
        Subscription {
            topic: Some(topic.parse::<UUri>().expect("valid topic URI")).into(),
            subscriber: Some(SubscriberInfo {
                uri: Some(subscriber.parse::<UUri>().expect("valid subscriber URI")).into(),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        }
    }

    fn valid_snapshot() -> FetchSubscriptionsResponse {
        FetchSubscriptionsResponse {
            subscriptions: vec![subscription(
                "//authority-a/5BA0/1/8001",
                "//authority-b/5678/1/1234",
            )],
            ..Default::default()
        }
    }

    fn invalid_snapshot_missing_topic() -> FetchSubscriptionsResponse {
        FetchSubscriptionsResponse {
            subscriptions: vec![Subscription {
                topic: None.into(),
                subscriber: Some(SubscriberInfo {
                    uri: Some("//authority-b/5678/1/1234".parse::<UUri>().unwrap()).into(),
                    ..Default::default()
                })
                .into(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn subscription_sync_health_default_is_empty() {
        let health = SubscriptionSyncHealth::default();
        assert_eq!(health.last_attempt_at, None);
        assert_eq!(health.last_success_at, None);
        assert_eq!(health.last_attempt_succeeded, None);
        assert_eq!(health.previous_attempt_succeeded, None);
    }

    #[tokio::test]
    async fn startup_fetch_failure_is_non_fatal_and_sets_first_failed_attempt() {
        let usubscription: Arc<dyn USubscription> =
            Arc::new(SequencedUSubscription::new(vec![Err(
                UStatus::fail_with_code(UCode::UNAVAILABLE, "simulated bootstrap failure"),
            )]));

        let streamer = UStreamer::new("startup-failure", 16, usubscription)
            .await
            .expect("startup should not fail when snapshot fetch fails");

        let health = streamer.subscription_sync_health();
        assert!(health.last_attempt_at.is_some());
        assert_eq!(health.last_success_at, None);
        assert_eq!(health.last_attempt_succeeded, Some(false));
        assert_eq!(health.previous_attempt_succeeded, None);
    }

    #[tokio::test]
    async fn refresh_success_returns_health_and_rolls_previous_attempt_state() {
        let usubscription: Arc<dyn USubscription> = Arc::new(SequencedUSubscription::new(vec![
            Err(UStatus::fail_with_code(
                UCode::UNAVAILABLE,
                "first bootstrap failure",
            )),
            Ok(valid_snapshot()),
        ]));

        let mut streamer = UStreamer::new("refresh-success", 16, usubscription)
            .await
            .expect("startup should be non-fatal");

        let returned_health = streamer
            .refresh_subscriptions()
            .await
            .expect("refresh should succeed");
        let accessor_health = streamer.subscription_sync_health();

        assert_eq!(returned_health, accessor_health);
        assert!(returned_health.last_attempt_at.is_some());
        assert!(returned_health.last_success_at.is_some());
        assert_eq!(returned_health.last_attempt_succeeded, Some(true));
        assert_eq!(returned_health.previous_attempt_succeeded, Some(false));
    }

    #[tokio::test]
    async fn failed_refresh_updates_health_visible_via_accessor() {
        let usubscription: Arc<dyn USubscription> = Arc::new(SequencedUSubscription::new(vec![
            Ok(valid_snapshot()),
            Err(UStatus::fail_with_code(
                UCode::UNAVAILABLE,
                "refresh failure",
            )),
        ]));

        let mut streamer = UStreamer::new("refresh-failure", 16, usubscription)
            .await
            .expect("startup should succeed");

        assert!(streamer.refresh_subscriptions().await.is_err());

        let health = streamer.subscription_sync_health();
        assert!(health.last_attempt_at.is_some());
        assert!(health.last_success_at.is_some());
        assert_eq!(health.last_attempt_succeeded, Some(false));
        assert_eq!(health.previous_attempt_succeeded, Some(true));
    }

    #[tokio::test]
    async fn failed_refresh_apply_marks_attempt_failed_after_prior_success() {
        let usubscription: Arc<dyn USubscription> = Arc::new(SequencedUSubscription::new(vec![
            Ok(valid_snapshot()),
            Ok(invalid_snapshot_missing_topic()),
        ]));

        let mut streamer = UStreamer::new("refresh-apply-failure", 16, usubscription)
            .await
            .expect("startup should succeed");

        let refresh_result = streamer.refresh_subscriptions().await;
        assert!(refresh_result.is_err());

        let health = streamer.subscription_sync_health();
        assert_eq!(health.last_attempt_succeeded, Some(false));
        assert_eq!(health.previous_attempt_succeeded, Some(true));
        assert!(health.last_success_at.is_some());
    }
}

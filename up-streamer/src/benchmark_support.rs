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

//! Deterministic benchmark fixtures for the Criterion harness.

use crate::data_plane::egress_worker::EgressRouteWorker;
use crate::data_plane::ingress_registry::IngressRouteRegistry;
use crate::routing::publish_resolution::PublishRouteResolver;
use crate::routing::subscription_directory::SubscriptionDirectory;
use async_trait::async_trait;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use up_rust::core::usubscription::{FetchSubscriptionsResponse, SubscriberInfo, Subscription};
use up_rust::{UCode, UListener, UMessage, UStatus, UTransport, UUri};

fn subscription(topic: UUri, subscriber: UUri) -> Subscription {
    Subscription {
        topic: Some(topic).into(),
        subscriber: Some(SubscriberInfo {
            uri: Some(subscriber).into(),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    }
}

fn topic_uri(authority: &str, index: usize) -> UUri {
    let ue_id = 0x5BA0 + (index as u32 % 0x1F0);
    let resource_id = 0x8001 + (index as u16 % 0x01F0);

    if authority == "*" {
        return UUri::from_str(&format!("//*/{ue_id:X}/1/{resource_id:X}"))
            .expect("wildcard topic URI should parse");
    }

    UUri::try_from_parts(authority, ue_id, 0x1, resource_id).expect("topic URI should build")
}

fn subscriber_uri(authority: &str, index: usize) -> UUri {
    let ue_id = 0x7000 + (index as u32 % 0x200);
    let resource_id = 0x9000 + (index as u16 % 0x200);

    if authority == "*" {
        return UUri::from_str(&format!("//*/{ue_id:X}/1/{resource_id:X}"))
            .expect("wildcard subscriber URI should parse");
    }

    UUri::try_from_parts(authority, ue_id, 0x1, resource_id).expect("subscriber URI should build")
}

async fn directory_from_subscriptions(
    subscriptions: Vec<Subscription>,
) -> Result<SubscriptionDirectory, UStatus> {
    let directory = SubscriptionDirectory::empty();
    directory
        .apply_snapshot(FetchSubscriptionsResponse {
            subscriptions,
            ..Default::default()
        })
        .await?;
    Ok(directory)
}

fn build_subscriptions(
    topic_authority: &str,
    subscriber_authority: &str,
    rows: usize,
) -> Vec<Subscription> {
    let total_rows = rows.max(1);
    let mut subscriptions = Vec::with_capacity(total_rows);

    for index in 0..total_rows {
        subscriptions.push(subscription(
            topic_uri(topic_authority, index),
            subscriber_uri(subscriber_authority, index),
        ));
    }

    subscriptions
}

/// Fixed fixture for `routing_lookup/*` benchmark IDs.
pub struct RoutingLookupFixture {
    directory: SubscriptionDirectory,
    out_authority: String,
}

impl RoutingLookupFixture {
    pub async fn exact_authority(rows: usize) -> Result<Self, UStatus> {
        let out_authority = "authority-b".to_string();
        let subscriptions = build_subscriptions("authority-a", &out_authority, rows);
        let directory = directory_from_subscriptions(subscriptions).await?;

        Ok(Self {
            directory,
            out_authority,
        })
    }

    pub async fn wildcard_authority(rows: usize) -> Result<Self, UStatus> {
        let out_authority = "authority-b".to_string();
        let subscriptions = build_subscriptions("authority-a", "*", rows);
        let directory = directory_from_subscriptions(subscriptions).await?;

        Ok(Self {
            directory,
            out_authority,
        })
    }

    pub async fn lookup_count(&self) -> usize {
        self.directory
            .lookup_route_subscribers_with_version(&self.out_authority)
            .await
            .1
            .len()
    }
}

/// Fixed fixture for `publish_resolution/source_filter_derivation` benchmark ID.
pub struct PublishResolutionFixture {
    subscribers: crate::routing::subscription_cache::SubscriptionLookup,
    ingress_authority: String,
    egress_authority: String,
}

impl PublishResolutionFixture {
    pub async fn new(rows: usize) -> Result<Self, UStatus> {
        let ingress_authority = "authority-a".to_string();
        let egress_authority = "authority-b".to_string();
        let total_rows = rows.max(1);
        let mut subscriptions = Vec::with_capacity(total_rows);

        for index in 0..total_rows {
            let topic_authority = if index % 3 == 0 {
                "*"
            } else {
                ingress_authority.as_str()
            };
            let topic = topic_uri(topic_authority, index % 64);
            let subscriber = subscriber_uri(&egress_authority, index);
            subscriptions.push(subscription(topic, subscriber));
        }

        let directory = directory_from_subscriptions(subscriptions).await?;
        let subscribers = directory
            .lookup_route_subscribers_with_version(&egress_authority)
            .await
            .1;

        Ok(Self {
            subscribers,
            ingress_authority,
            egress_authority,
        })
    }

    pub fn derive_source_filter_count(&self) -> usize {
        PublishRouteResolver::derive_source_filters(
            &self.ingress_authority,
            &self.egress_authority,
            &self.subscribers,
        )
        .len()
    }
}

struct NoopRegistryTransport;

#[async_trait]
impl UTransport for NoopRegistryTransport {
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
            "receive is not used by benchmark fixture",
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

/// Fixed fixture for `ingress_registry/*` benchmark IDs.
pub struct IngressRegistryFixture {
    registry: IngressRouteRegistry,
    in_transport: Arc<dyn UTransport>,
    out_sender: broadcast::Sender<Arc<UMessage>>,
    directory: SubscriptionDirectory,
    ingress_authority: String,
    egress_authority: String,
    route_id: String,
}

impl IngressRegistryFixture {
    pub async fn new(subscription_rows: usize) -> Result<Self, UStatus> {
        let ingress_authority = "authority-a".to_string();
        let egress_authority = "authority-b".to_string();
        let directory = directory_from_subscriptions(build_subscriptions(
            &ingress_authority,
            &egress_authority,
            subscription_rows,
        ))
        .await?;
        let (out_sender, _) = broadcast::channel(64);

        Ok(Self {
            registry: IngressRouteRegistry::new(),
            in_transport: Arc::new(NoopRegistryTransport),
            out_sender,
            directory,
            ingress_authority,
            egress_authority,
            route_id: "benchmark-route".to_string(),
        })
    }

    pub async fn register_route(&self) -> bool {
        self.registry
            .register_route(
                self.in_transport.clone(),
                &self.ingress_authority,
                &self.egress_authority,
                &self.route_id,
                self.out_sender.clone(),
                &self.directory,
            )
            .await
            .is_ok()
    }

    pub async fn unregister_route(&self) {
        self.registry
            .unregister_route(
                self.in_transport.clone(),
                &self.ingress_authority,
                &self.egress_authority,
                &self.directory,
            )
            .await;
    }
}

#[derive(Default)]
struct CountingDispatchTransport {
    send_count: AtomicUsize,
}

impl CountingDispatchTransport {
    fn sent_count(&self) -> usize {
        self.send_count.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl UTransport for CountingDispatchTransport {
    async fn send(&self, _message: UMessage) -> Result<(), UStatus> {
        self.send_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn receive(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
    ) -> Result<UMessage, UStatus> {
        Err(UStatus::fail_with_code(
            UCode::UNIMPLEMENTED,
            "receive is not used by benchmark fixture",
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

/// Executes one in-process egress dispatch cycle and returns the send count.
pub async fn run_single_route_dispatch_once() -> usize {
    let dispatch_transport = Arc::new(CountingDispatchTransport::default());
    let out_transport: Arc<dyn UTransport> = dispatch_transport.clone();

    let (sender, receiver) = broadcast::channel(8);
    sender
        .send(Arc::new(UMessage::default()))
        .expect("dispatch channel should accept one message");
    drop(sender);

    EgressRouteWorker::route_dispatch_loop(
        "benchmark-egress-dispatch".to_string(),
        out_transport,
        receiver,
    )
    .await;

    dispatch_transport.sent_count()
}

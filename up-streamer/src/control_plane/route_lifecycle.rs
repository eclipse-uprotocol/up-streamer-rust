//! Route lifecycle orchestration for control-plane route transitions.

use crate::control_plane::route_table::{RouteKey, RouteTable};
use crate::data_plane::egress_pool::EgressRoutePool;
use crate::data_plane::ingress_registry::{IngressRouteRegistrationError, IngressRouteRegistry};
use crate::endpoint::Endpoint;
use crate::routing::subscription_directory::SubscriptionDirectory;

/// Failures for route insertion orchestration.
pub(crate) enum AddRouteError {
    SameAuthority,
    AlreadyExists,
    FailedToRegisterIngressRoute(IngressRouteRegistrationError),
}

/// Failures for route deletion orchestration.
pub(crate) enum RemoveRouteError {
    SameAuthority,
    NotFound,
}

/// Orchestrates multi-owner route transitions across control, routing, and data planes.
pub(crate) struct RouteLifecycle<'a> {
    route_table: &'a RouteTable,
    egress_route_pool: &'a mut EgressRoutePool,
    ingress_route_registry: &'a IngressRouteRegistry,
    subscription_directory: &'a SubscriptionDirectory,
}

impl<'a> RouteLifecycle<'a> {
    /// Creates a lifecycle coordinator using existing domain owners.
    pub(crate) fn new(
        route_table: &'a RouteTable,
        egress_route_pool: &'a mut EgressRoutePool,
        ingress_route_registry: &'a IngressRouteRegistry,
        subscription_directory: &'a SubscriptionDirectory,
    ) -> Self {
        Self {
            route_table,
            egress_route_pool,
            ingress_route_registry,
            subscription_directory,
        }
    }

    /// Registers a route and applies rollback when ingress registration fails.
    pub(crate) async fn add_route(
        &mut self,
        r#in: &Endpoint,
        out: &Endpoint,
        route_label: &str,
    ) -> Result<(), AddRouteError> {
        if r#in.authority == out.authority {
            return Err(AddRouteError::SameAuthority);
        }

        let route_key = RouteKey::from_endpoints(r#in, out);
        let inserted = self.route_table.insert_route(route_key.clone()).await;

        if !inserted {
            return Err(AddRouteError::AlreadyExists);
        }

        let out_sender = self
            .egress_route_pool
            .attach_route(out.transport.clone())
            .await;

        if let Err(err) = self
            .ingress_route_registry
            .register_route(
                r#in.transport.clone(),
                &r#in.authority,
                &out.authority,
                route_label,
                out_sender,
                self.subscription_directory,
            )
            .await
        {
            self.route_table.remove_route(&route_key).await;
            self.egress_route_pool
                .detach_route(out.transport.clone())
                .await;

            return Err(AddRouteError::FailedToRegisterIngressRoute(err));
        }

        Ok(())
    }

    /// Removes a route and detaches ingress/egress resources when present.
    pub(crate) async fn remove_route(
        &mut self,
        r#in: &Endpoint,
        out: &Endpoint,
    ) -> Result<(), RemoveRouteError> {
        if r#in.authority == out.authority {
            return Err(RemoveRouteError::SameAuthority);
        }

        let route_key = RouteKey::from_endpoints(r#in, out);
        let removed = self.route_table.remove_route(&route_key).await;

        if !removed {
            return Err(RemoveRouteError::NotFound);
        }

        self.egress_route_pool
            .detach_route(out.transport.clone())
            .await;
        self.ingress_route_registry
            .unregister_route(
                r#in.transport.clone(),
                &r#in.authority,
                &out.authority,
                self.subscription_directory,
            )
            .await;

        Ok(())
    }
}

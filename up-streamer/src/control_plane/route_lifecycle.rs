//! Route lifecycle orchestration for control-plane route transitions.

use crate::control_plane::route_table::{RouteKey, RouteTable};
use crate::data_plane::egress_pool::EgressRoutePool;
use crate::data_plane::ingress_registry::{IngressRouteRegistrationError, IngressRouteRegistry};
use crate::endpoint::Endpoint;
use crate::routing::subscription_directory::SubscriptionDirectory;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Failures for route insertion orchestration.
#[derive(Debug)]
pub(crate) enum AddRouteError {
    SameAuthority,
    AlreadyExists,
    FailedToRegisterIngressRoute(IngressRouteRegistrationError),
}

/// Failures for route deletion orchestration.
#[derive(Debug)]
pub(crate) enum RemoveRouteError {
    SameAuthority,
    NotFound,
}

impl Display for AddRouteError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            AddRouteError::SameAuthority => {
                write!(f, "ingress and egress authorities must differ")
            }
            AddRouteError::AlreadyExists => write!(f, "route already exists"),
            AddRouteError::FailedToRegisterIngressRoute(err) => {
                write!(f, "failed to register ingress route listener: {err}")
            }
        }
    }
}

impl Error for AddRouteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            AddRouteError::FailedToRegisterIngressRoute(err) => Some(err),
            _ => None,
        }
    }
}

impl Display for RemoveRouteError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RemoveRouteError::SameAuthority => {
                write!(f, "ingress and egress authorities must differ")
            }
            RemoveRouteError::NotFound => write!(f, "route not found"),
        }
    }
}

impl Error for RemoveRouteError {}

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

#[cfg(test)]
mod tests {
    use super::{AddRouteError, RemoveRouteError};
    use crate::data_plane::ingress_registry::IngressRouteRegistrationError;
    use std::error::Error;

    #[test]
    fn add_route_error_exposes_display_and_source_for_ingress_failure() {
        let error = AddRouteError::FailedToRegisterIngressRoute(
            IngressRouteRegistrationError::FailedToRegisterRequestRouteListener,
        );

        assert!(error
            .to_string()
            .contains("failed to register ingress route listener"));
        assert!(error.source().is_some());
    }

    #[test]
    fn remove_route_error_display_is_stable_for_not_found() {
        let error = RemoveRouteError::NotFound;

        assert_eq!(error.to_string(), "route not found");
        assert!(error.source().is_none());
    }
}

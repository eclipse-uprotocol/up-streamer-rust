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

//! Runtime helper for spawning egress route dispatch loops.

use crate::observability::{events, fields};
use std::sync::Arc;
use std::thread;
use tokio::runtime::{Builder, Handle};
use tokio::sync::broadcast::Receiver;
use tokio::task::JoinHandle as TokioJoinHandle;
use tracing::{debug, error, warn};
use up_rust::{UMessage, UTransport};

const LINUX_THREAD_NAME_MAX_LEN: usize = 15;
pub(crate) const DEFAULT_EGRESS_ROUTE_RUNTIME_THREAD_NAME: &str = "up-egress-route";
const COMPONENT: &str = "worker_runtime";

/// Runtime handle backing one egress dispatch loop.
pub(crate) enum RouteDispatchLoopHandle {
    SharedTask {
        worker_thread: String,
        _join_handle: TokioJoinHandle<()>,
    },
    DedicatedThread {
        worker_thread: String,
        _join_handle: thread::JoinHandle<()>,
    },
}

impl RouteDispatchLoopHandle {
    pub(crate) fn worker_thread(&self) -> &str {
        match self {
            Self::SharedTask { worker_thread, .. } => worker_thread,
            Self::DedicatedThread { worker_thread, .. } => worker_thread,
        }
    }
}

fn sanitize_runtime_thread_name(thread_name: String) -> String {
    if thread_name.is_empty() || thread_name.len() > LINUX_THREAD_NAME_MAX_LEN {
        warn!(
            event = events::RUNTIME_THREAD_NAME_FALLBACK,
            component = COMPONENT,
            reason = fields::REASON_INVALID_THREAD_NAME,
            worker_thread = thread_name.as_str(),
            "invalid runtime thread name; using fallback"
        );
        DEFAULT_EGRESS_ROUTE_RUNTIME_THREAD_NAME.to_string()
    } else {
        thread_name
    }
}

pub(crate) fn spawn_route_dispatch_loop<F, Fut>(
    thread_name: String,
    out_transport: Arc<dyn UTransport>,
    message_receiver: Receiver<Arc<UMessage>>,
    run_loop: F,
) -> RouteDispatchLoopHandle
where
    F: FnOnce(Arc<dyn UTransport>, Receiver<Arc<UMessage>>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let runtime_thread_name = sanitize_runtime_thread_name(thread_name);
    debug!(
        event = events::RUNTIME_SPAWN_START,
        component = COMPONENT,
        worker_thread = runtime_thread_name.as_str(),
        "spawning egress route runtime thread"
    );

    if let Ok(runtime_handle) = Handle::try_current() {
        let join_handle = runtime_handle.spawn(run_loop(out_transport, message_receiver));
        debug!(
            event = events::RUNTIME_SPAWN_OK,
            component = COMPONENT,
            worker_thread = runtime_thread_name.as_str(),
            runtime_strategy = "shared_runtime_task",
            "spawned egress route runtime task"
        );
        return RouteDispatchLoopHandle::SharedTask {
            worker_thread: runtime_thread_name,
            _join_handle: join_handle,
        };
    }

    let builder_thread_name = runtime_thread_name.clone();
    let spawn_result = thread::Builder::new()
        .name(builder_thread_name)
        .spawn(move || {
            let runtime = match Builder::new_current_thread().enable_all().build() {
                Ok(runtime) => runtime,
                Err(err) => {
                    error!(
                            event = events::RUNTIME_SPAWN_FAILED,
                            component = COMPONENT,
                            worker_thread = std::thread::current()
                                .name()
                                .unwrap_or(DEFAULT_EGRESS_ROUTE_RUNTIME_THREAD_NAME),
                            reason = "tokio_runtime_build_failed",
                            err = %err,
                    "failed to build egress route runtime"
                        );
                    panic!("Failed to create egress route Tokio runtime: {err}");
                }
            };

            runtime.block_on(run_loop(out_transport, message_receiver));
        });

    match spawn_result {
        Ok(join_handle) => {
            debug!(
                event = events::RUNTIME_SPAWN_OK,
                component = COMPONENT,
                worker_thread = runtime_thread_name.as_str(),
                runtime_strategy = "dedicated_thread",
                "spawned egress route runtime thread"
            );
            RouteDispatchLoopHandle::DedicatedThread {
                worker_thread: runtime_thread_name,
                _join_handle: join_handle,
            }
        }
        Err(err) => {
            error!(
                event = events::RUNTIME_SPAWN_FAILED,
                component = COMPONENT,
                worker_thread = runtime_thread_name.as_str(),
                reason = "thread_spawn_failed",
                err = %err,
                "failed to spawn egress route runtime thread"
            );
            panic!("Failed to spawn egress route runtime thread: {err}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{sanitize_runtime_thread_name, DEFAULT_EGRESS_ROUTE_RUNTIME_THREAD_NAME};

    #[test]
    fn sanitize_runtime_thread_name_keeps_valid_name() {
        let name = sanitize_runtime_thread_name("up-egress-12345".to_string());
        assert_eq!(name, "up-egress-12345");
    }

    #[test]
    fn sanitize_runtime_thread_name_uses_fallback_for_empty_name() {
        let name = sanitize_runtime_thread_name(String::new());
        assert_eq!(name, DEFAULT_EGRESS_ROUTE_RUNTIME_THREAD_NAME);
    }

    #[test]
    fn sanitize_runtime_thread_name_uses_fallback_for_long_name() {
        let name = sanitize_runtime_thread_name("up-egress-thread-name-too-long".to_string());
        assert_eq!(name, DEFAULT_EGRESS_ROUTE_RUNTIME_THREAD_NAME);
    }
}

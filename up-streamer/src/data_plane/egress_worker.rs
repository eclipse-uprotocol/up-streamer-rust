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

//! Egress worker abstraction that forwards queued messages on output transports.

use crate::runtime::worker_runtime::{
    spawn_route_dispatch_loop, RouteDispatchLoopHandle, DEFAULT_EGRESS_ROUTE_RUNTIME_THREAD_NAME,
};
use crate::{
    observability::events,
    observability::fields::{self, WorkerContext},
};
use std::ops::Deref;
use std::sync::Arc;
use tokio::sync::broadcast::{error::RecvError, Receiver};
use tracing::{debug, info, warn, Level};
use up_rust::{UMessage, UTransport, UUID};

const EGRESS_ROUTE_RUNTIME_THREAD_NAME_PREFIX: &str = "up-egress-";
const EGRESS_ROUTE_RUNTIME_THREAD_NAME_MAX_LEN: usize = 15;
const COMPONENT: &str = "egress_worker";

struct FormattedMessageFields {
    msg_id: String,
    msg_type: String,
    src: String,
    sink: String,
}

impl FormattedMessageFields {
    fn from_message(message: &UMessage) -> Self {
        Self {
            msg_id: fields::format_message_id(message),
            msg_type: fields::format_message_type(message),
            src: fields::format_source_uri(message),
            sink: fields::format_sink_uri(message),
        }
    }
}

/// Worker state that owns the spawned route-dispatch thread handle.
pub(crate) struct EgressRouteWorker {
    worker_id: String,
    dispatch_handle: RouteDispatchLoopHandle,
}

impl EgressRouteWorker {
    /// Spawns a dedicated runtime thread for one egress transport dispatch loop.
    pub(crate) fn new(
        out_transport: Arc<dyn UTransport>,
        message_receiver: Receiver<Arc<UMessage>>,
    ) -> Self {
        let out_transport_clone = out_transport.clone();
        let worker_id = UUID::build().to_hyphenated_string();
        let runtime_thread_name = Self::build_runtime_thread_name(&worker_id);
        let worker_id_for_loop = worker_id.clone();

        let dispatch_handle = spawn_route_dispatch_loop(
            runtime_thread_name,
            out_transport_clone,
            message_receiver,
            move |out_transport, message_receiver| async move {
                Self::route_dispatch_loop(worker_id_for_loop, out_transport, message_receiver)
                    .await;
            },
        );

        Self {
            worker_id,
            dispatch_handle,
        }
    }

    /// Returns the unique worker identifier for correlation logs.
    pub(crate) fn worker_id(&self) -> &str {
        &self.worker_id
    }

    /// Returns the worker runtime thread label for diagnostics.
    pub(crate) fn runtime_thread(&self) -> &str {
        self.dispatch_handle.worker_thread()
    }

    fn build_runtime_thread_name(route_id: &str) -> String {
        let suffix_len = EGRESS_ROUTE_RUNTIME_THREAD_NAME_MAX_LEN
            - EGRESS_ROUTE_RUNTIME_THREAD_NAME_PREFIX.len();
        let suffix: String = route_id
            .chars()
            .filter(|ch| ch.is_ascii_hexdigit())
            .take(suffix_len)
            .collect();

        if suffix.len() == suffix_len {
            format!("{EGRESS_ROUTE_RUNTIME_THREAD_NAME_PREFIX}{suffix}")
        } else {
            DEFAULT_EGRESS_ROUTE_RUNTIME_THREAD_NAME.to_string()
        }
    }

    /// Executes the dispatch loop by forwarding each received message to egress transport.
    pub(crate) async fn route_dispatch_loop(
        worker_id: String,
        out_transport: Arc<dyn UTransport>,
        mut message_receiver: Receiver<Arc<UMessage>>,
    ) {
        let worker_context = WorkerContext::with_current_thread(worker_id);

        loop {
            match message_receiver.recv().await {
                Ok(msg) => {
                    let message = msg.deref();
                    let mut message_fields = tracing::enabled!(Level::DEBUG)
                        .then(|| FormattedMessageFields::from_message(message));

                    if let Some(fields) = message_fields.as_ref() {
                        debug!(
                            event = events::EGRESS_SEND_ATTEMPT,
                            component = COMPONENT,
                            worker_id = worker_context.worker_id.as_str(),
                            worker_thread = worker_context.worker_thread.as_str(),
                            msg_id = fields.msg_id.as_str(),
                            msg_type = fields.msg_type.as_str(),
                            src = fields.src.as_str(),
                            sink = fields.sink.as_str(),
                            "attempting egress send"
                        );
                    }

                    let send_res = out_transport.send(message.clone()).await;
                    if let Err(err) = send_res {
                        if tracing::enabled!(Level::WARN) {
                            let fields = message_fields.get_or_insert_with(|| {
                                FormattedMessageFields::from_message(message)
                            });

                            warn!(
                                event = events::EGRESS_SEND_FAILED,
                                component = COMPONENT,
                                worker_id = worker_context.worker_id.as_str(),
                                worker_thread = worker_context.worker_thread.as_str(),
                                msg_id = fields.msg_id.as_str(),
                                msg_type = fields.msg_type.as_str(),
                                src = fields.src.as_str(),
                                sink = fields.sink.as_str(),
                                err = ?err,
                                "egress send failed"
                            );
                        }
                    } else if let Some(fields) = message_fields.as_ref() {
                        debug!(
                            event = events::EGRESS_SEND_OK,
                            component = COMPONENT,
                            worker_id = worker_context.worker_id.as_str(),
                            worker_thread = worker_context.worker_thread.as_str(),
                            msg_id = fields.msg_id.as_str(),
                            msg_type = fields.msg_type.as_str(),
                            src = fields.src.as_str(),
                            sink = fields.sink.as_str(),
                            "egress send succeeded"
                        );
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    warn!(
                        event = events::EGRESS_RECV_LAGGED,
                        component = COMPONENT,
                        worker_id = worker_context.worker_id.as_str(),
                        worker_thread = worker_context.worker_thread.as_str(),
                        skipped,
                        "receiver lagged"
                    );
                }
                Err(RecvError::Closed) => {
                    info!(
                        event = events::EGRESS_RECV_CLOSED,
                        component = COMPONENT,
                        worker_id = worker_context.worker_id.as_str(),
                        worker_thread = worker_context.worker_thread.as_str(),
                        reason = fields::REASON_BROADCAST_CLOSED,
                        "receiver closed; stopping dispatch loop"
                    );
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EgressRouteWorker, EGRESS_ROUTE_RUNTIME_THREAD_NAME_MAX_LEN,
        EGRESS_ROUTE_RUNTIME_THREAD_NAME_PREFIX,
    };
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::broadcast;
    use up_rust::{UCode, UListener, UMessage, UStatus, UTransport, UUri};

    #[derive(Default)]
    struct CountingTransport {
        send_count: AtomicUsize,
    }

    impl CountingTransport {
        fn sent_count(&self) -> usize {
            self.send_count.load(Ordering::Relaxed)
        }
    }

    #[async_trait]
    impl UTransport for CountingTransport {
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
                "receive is not used by egress worker tests",
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

    #[tokio::test]
    async fn route_dispatch_loop_exits_on_closed_receiver() {
        let transport = Arc::new(CountingTransport::default());
        let out_transport: Arc<dyn UTransport> = transport.clone();
        let (sender, receiver) = broadcast::channel(8);
        drop(sender);

        EgressRouteWorker::route_dispatch_loop("closed-loop".to_string(), out_transport, receiver)
            .await;

        assert_eq!(transport.sent_count(), 0);
    }

    #[tokio::test]
    async fn route_dispatch_loop_does_not_forward_after_close() {
        let transport = Arc::new(CountingTransport::default());
        let out_transport: Arc<dyn UTransport> = transport.clone();
        let (sender, receiver) = broadcast::channel(8);

        sender
            .send(Arc::new(UMessage::default()))
            .expect("queue should accept pre-close message");
        drop(sender);

        EgressRouteWorker::route_dispatch_loop(
            "close-forwarding".to_string(),
            out_transport,
            receiver,
        )
        .await;

        assert_eq!(transport.sent_count(), 1);
    }

    #[tokio::test]
    async fn route_dispatch_loop_continues_after_lagged_receive() {
        let transport = Arc::new(CountingTransport::default());
        let out_transport: Arc<dyn UTransport> = transport.clone();
        let (sender, receiver) = broadcast::channel(1);

        sender
            .send(Arc::new(UMessage::default()))
            .expect("queue should accept first message");
        sender
            .send(Arc::new(UMessage::default()))
            .expect("queue should accept second message");
        drop(sender);

        EgressRouteWorker::route_dispatch_loop("lagged-loop".to_string(), out_transport, receiver)
            .await;

        assert_eq!(transport.sent_count(), 1);
    }

    #[test]
    fn build_runtime_thread_name_keeps_prefix_and_linux_safe_length() {
        let thread_name = EgressRouteWorker::build_runtime_thread_name("abcdef0123456789");

        assert!(thread_name.starts_with(EGRESS_ROUTE_RUNTIME_THREAD_NAME_PREFIX));
        assert_eq!(thread_name.len(), EGRESS_ROUTE_RUNTIME_THREAD_NAME_MAX_LEN);
    }

    #[test]
    fn build_runtime_thread_name_uses_fallback_for_short_non_hex_ids() {
        let thread_name = EgressRouteWorker::build_runtime_thread_name("zzz");

        assert_eq!(
            thread_name,
            crate::runtime::worker_runtime::DEFAULT_EGRESS_ROUTE_RUNTIME_THREAD_NAME
        );
    }
}

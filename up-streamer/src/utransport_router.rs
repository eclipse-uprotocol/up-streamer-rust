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

use crate::utransport_builder::UTransportBuilder;
use async_std::channel::{bounded, Receiver, Sender};
use async_std::future::timeout;
use async_std::sync::{Arc, Mutex};
use async_std::task;
use futures::select;
use futures::FutureExt;
use log::*;
use std::any::Any;
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;
use std::{fmt, thread};
use up_rust::{UAuthority, UCode, UMessage, UStatus, UTransport, UUIDBuilder, UUri, UUID};

const UTRANSPORT_ROUTER_TAG: &str = "UTransportRouter";
const UTRANSPORT_ROUTER_FN_START_TAG: &str = "start()";

const UTRANSPORT_ROUTER_INNER_TAG: &str = "UTransportRouterInner:";
const UTRANSPORT_ROUTER_INNER_FN_START_TAG: &str = "start()";
const UTRANSPORT_ROUTER_INNER_FN_LAUNCH_TAG: &str = "launch()";
const UTRANSPORT_ROUTER_INNER_FN_HANDLE_COMMAND_TAG: &str = "handle_command():";
const UTRANSPORT_ROUTER_INNER_FN_SEND_OVER_TRANSPORT_TAG: &str = "send_over_utransport():";
const UTRANSPORT_ROUTER_INNER_FN_FORWARDING_CALLBACK_TAG: &str = "forwarding_callback():";

const UTRANSPORT_ROUTER_HANDLE_TAG: &str = "UTransportRouterHandle:";
const UTRANSPORT_ROUTER_HANDLE_FN_REGISTER_TAG: &str = "register():";
const UTRANSPORT_ROUTER_HANDLE_FN_UNREGISTER_TAG: &str = "unregister():";

#[derive(Clone)]
pub(crate) struct ComparableSender<T> {
    pub(crate) id: UUID,
    sender: Arc<Sender<T>>,
}

impl<T> Debug for ComparableSender<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SenderWrapper {{ id: {} }}", self.id)
    }
}

impl<T> ComparableSender<T> {
    pub fn new(sender: Sender<T>) -> Self {
        let id = UUIDBuilder::new().build();
        let sender = Arc::new(sender);
        Self { id, sender }
    }
}

impl<T> Deref for ComparableSender<T> {
    type Target = Sender<T>;

    fn deref(&self) -> &Self::Target {
        &self.sender
    }
}

impl<T> Hash for ComparableSender<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T> PartialEq for ComparableSender<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for ComparableSender<T> {}

/// A [`UTransportRouterHandle`] which is returned from starting a [`UTransportRouter`]
///
/// Used to build a [`Route`][crate::Route]
pub struct UTransportRouterHandle {
    pub(crate) name: String,
    pub(crate) command_sender: Sender<UTransportRouterCommand>,
    pub(crate) message_sender: ComparableSender<UMessage>,
}

impl UTransportRouterHandle {
    pub(crate) async fn register(
        &self,
        in_authority: UAuthority,
        out_authority: UAuthority,
        out_sender_wrapper: ComparableSender<UMessage>,
    ) -> Result<(), UStatus> {
        if log_enabled!(Level::Debug) {
            debug!(
                "{}:{}:{} Sending registration request for: ({:?}, {:?}, {})",
                &self.name,
                &UTRANSPORT_ROUTER_HANDLE_TAG,
                &UTRANSPORT_ROUTER_HANDLE_FN_REGISTER_TAG,
                &in_authority,
                &out_authority,
                &out_sender_wrapper.id
            );
        }
        let (tx_result, rx_result) = bounded(1);
        match self
            .command_sender
            .send(UTransportRouterCommand::Register(
                RegisterUnregisterControl {
                    in_authority,
                    out_authority,
                    out_sender_wrapper,
                    result_sender: tx_result,
                },
            ))
            .await
        {
            Ok(_) => {}
            Err(e) => {
                return Err(UStatus::fail_with_code(
                    UCode::INTERNAL,
                    format!("{}: Unable to forward: {:?}", &self.name, e),
                ));
            }
        }

        let timeout_duration = Duration::from_millis(1000); // Example: 5 seconds
        match timeout(timeout_duration, rx_result.recv()).await {
            Ok(result) => match result {
                Ok(result) => match result {
                    Ok(_) => Ok(()),
                    Err(e) => Err(e),
                },
                Err(e) => {
                    // The channel was closed before a message was received.
                    Err(UStatus::fail_with_code(
                        UCode::INTERNAL,
                        format!(
                            "{}: Channel closed before receiving a response: {e:?}",
                            &self.name
                        ),
                    ))
                }
            },
            Err(_) => {
                // Timeout occurred
                Err(UStatus::fail_with_code(
                    UCode::INTERNAL,
                    format!("{}: Operation timed out", &self.name),
                ))
            }
        }
    }

    pub(crate) async fn unregister(
        &self,
        in_authority: UAuthority,
        out_authority: UAuthority,
        out_sender_wrapper: ComparableSender<UMessage>,
    ) -> Result<(), UStatus> {
        if log_enabled!(Level::Debug) {
            debug!(
                "{}:{}:{} Sending unregistration request for: ({:?}, {:?}, {})",
                &self.name,
                &UTRANSPORT_ROUTER_HANDLE_TAG,
                &UTRANSPORT_ROUTER_HANDLE_FN_UNREGISTER_TAG,
                &in_authority,
                &out_authority,
                &out_sender_wrapper.id
            );
        }
        let (tx_result, rx_result) = bounded(1);
        self.command_sender
            .send(UTransportRouterCommand::Unregister(
                RegisterUnregisterControl {
                    in_authority,
                    out_authority,
                    out_sender_wrapper,
                    result_sender: tx_result,
                },
            ))
            .await
            .map_err(|e| {
                UStatus::fail_with_code(
                    UCode::INTERNAL,
                    format!("{}: Unable to forward: {:?}", &self.name, e),
                )
            })?;

        let timeout_duration = Duration::from_millis(1000); // Example: 5 seconds
        match timeout(timeout_duration, rx_result.recv()).await {
            Ok(result) => match result {
                Ok(result) => match result {
                    Ok(_) => Ok(()),
                    Err(e) => Err(e),
                },
                Err(e) => {
                    // The channel was closed before a message was received.
                    Err(UStatus::fail_with_code(
                        UCode::INTERNAL,
                        format!(
                            "{}: Channel closed before receiving a response: {e:?}",
                            &self.name
                        ),
                    ))
                }
            },
            Err(_) => {
                // Timeout occurred
                Err(UStatus::fail_with_code(
                    UCode::INTERNAL,
                    format!("{}: Operation timed out", &self.name),
                ))
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct RegisterUnregisterControl {
    in_authority: UAuthority,
    out_authority: UAuthority,
    out_sender_wrapper: ComparableSender<UMessage>,
    result_sender: Sender<Result<(), UStatus>>,
}

#[derive(Debug)]
pub(crate) enum UTransportRouterCommand {
    Register(RegisterUnregisterControl),
    Unregister(RegisterUnregisterControl),
}

pub(crate) struct UTransportChannels {
    command_sender: Sender<UTransportRouterCommand>,
    command_receiver: Receiver<UTransportRouterCommand>,
    message_sender: ComparableSender<UMessage>,
    message_receiver: Receiver<UMessage>,
}

/// A [`UTransportRouter`] manages a `up-client-foo-rust`'s [`UTransport`][up_rust::UTransport]
/// implementation and returns a [`UTransportRouterHandle`] which is used by the [`UStreamer`][crate::UStreamer]
/// to communicate commands to the [`UTransportRouter`] for adding or removing routing rules.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use up_streamer::{Route, UTransportBuilder, UTransportRouter};
/// use up_rust::{Number, UAuthority};
///
/// # pub mod foo_transport_builder {
/// #     use up_rust::{UMessage, UTransport, UStatus, UUIDBuilder, UUri};
/// #     use async_trait::async_trait;
/// #     use up_streamer::UTransportBuilder;
/// #     pub struct UPClientFoo;
/// #
/// #     #[async_trait]
/// #     impl UTransport for UPClientFoo {
/// #         async fn send(&self, _message: UMessage) -> Result<(), UStatus> {
/// #             todo!()
/// #         }
/// #
/// #         async fn receive(&self, _topic: UUri) -> Result<UMessage, UStatus> {
/// #             todo!()
/// #         }
/// #
/// #         async fn register_listener(
/// #             &self,
/// #             topic: UUri,
/// #             _listener: Box<dyn Fn(Result<UMessage, UStatus>) + Send + Sync + 'static>,
/// #         ) -> Result<String, UStatus> {
/// #             println!("UPClientFoo: registering topic: {:?}", topic);
/// #             let uuid = UUIDBuilder::new().build();
/// #             Ok(uuid.to_string())
/// #         }
/// #
/// #         async fn unregister_listener(&self, topic: UUri, listener: &str) -> Result<(), UStatus> {
/// #             println!(
/// #                 "UPClientFoo: unregistering topic: {topic:?} with listener string: {listener}"
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
/// #     pub struct UTransportBuilderFoo;
/// #     impl UTransportBuilder for UTransportBuilderFoo {
/// #         fn build(&self) -> Result<Box<dyn UTransport>, UStatus> {
/// #             let utransport_foo: Box<dyn UTransport> = Box::new(UPClientFoo::new());
/// #             Ok(utransport_foo)
/// #         }
/// #     }
/// #
/// #     impl UTransportBuilderFoo {
/// #         pub fn new() -> Self {
/// #             Self {}
/// #         }
/// #     }
/// #
/// # }
///
/// let local_transport_router =
///             UTransportRouter::start("FOO".to_string(), foo_transport_builder::UTransportBuilderFoo::new(), 100, 200);
///         assert!(local_transport_router.is_ok());
///         let local_transport_router_handle = Arc::new(local_transport_router.unwrap());
/// ```
pub struct UTransportRouter {}

impl UTransportRouter {
    /// Starts the [`UTransportRouter`]
    ///
    /// # Parameters
    ///
    /// * `name` - Used for debugging and trace statements to disambiguate which [`UTransportRouter`]
    ///            is logging.
    /// * `utransport_builder` - A struct which implements [`UTransportBuilder`][crate::UTransportBuilder]
    /// * `command_queue_size` - The size of queue which can hold command messages from
    ///                          [`UTransportRouterHandle`] into [`UTransportRouter`]
    /// * `message_queue_size` - The size of queue which can hold messages intended to be
    ///                          sent onto the held `Box<dyn UTransport>`
    ///
    /// # Rationale
    ///
    /// We consume a struct which implements [`UTransportBuilder`][crate::UTransportBuilder] because
    /// [`UTransport`][up_rust::UTransport] is not thread-safe, meaning that we need to carefully
    /// manage on which thread we create the [`UTransport`][up_rust::UTransport]
    ///
    /// # Returns
    ///
    /// Returns a [`UTransportRouterHandle`] used internally by the [`UStreamer`][crate::UStreamer]
    /// to communicate commands to the [`UTransportRouter`] for adding or removing routing rules.
    ///
    /// # Errors
    ///
    /// Returns a [`UStatus`][up_rust::UStatus] if unsuccessful indicating the error which occurred.
    pub fn start<T>(
        name: String,
        utransport_builder: T,
        command_queue_size: usize,
        message_queue_size: usize,
    ) -> Result<UTransportRouterHandle, UStatus>
    where
        T: UTransportBuilder + 'static,
    {
        // Try to initiate logging.
        // Required in case of dynamic lib, otherwise no logs.
        // But cannot be done twice in case of static link.
        let _ = env_logger::try_init();

        let name = format!("{name}:{UTRANSPORT_ROUTER_TAG}");
        let (tx, rx) = mpsc::channel();

        if log_enabled!(Level::Debug) {
            debug!(
                "{}:{}:{} Starting UTransportRouter with this configuration: ({:?}, {}, {})",
                &name,
                &UTRANSPORT_ROUTER_TAG,
                &UTRANSPORT_ROUTER_FN_START_TAG,
                utransport_builder.type_id(),
                &command_queue_size,
                &message_queue_size
            );
        }

        let (command_sender, command_receiver) = bounded(command_queue_size);
        let (message_sender, message_receiver) = bounded(message_queue_size);
        let message_sender = ComparableSender::new(message_sender);

        let utransport_channels = UTransportChannels {
            command_sender: command_sender.clone(),
            command_receiver,
            message_sender: message_sender.clone(),
            message_receiver,
        };

        debug!(
            "{}:{}:{} Before starting UTransportRouterInner",
            &name, &UTRANSPORT_ROUTER_TAG, &UTRANSPORT_ROUTER_FN_START_TAG,
        );

        let name_clone = name.clone();
        let name_clone_clone = name_clone.clone();
        task::block_on(async move {
            debug!(
                "{}:{}:{} Inside task::block_on()",
                &name_clone_clone, &UTRANSPORT_ROUTER_TAG, &UTRANSPORT_ROUTER_FN_START_TAG,
            );
            let result = UTransportRouterInner::start(
                name_clone_clone.clone(),
                utransport_builder,
                utransport_channels,
            )
            .await;
            debug!(
                "{}:{}:{} After UTransportRouterInner::start()",
                &name_clone_clone, &UTRANSPORT_ROUTER_TAG, &UTRANSPORT_ROUTER_FN_START_TAG,
            );
            tx.send(result).unwrap();
            debug!(
                "{}:{}:{} After Transmitting result back from task",
                &name_clone_clone, &UTRANSPORT_ROUTER_TAG, &UTRANSPORT_ROUTER_FN_START_TAG,
            );
        });
        debug!(
            "{}:{}:{} Came back from task::block_on()",
            &name_clone, &UTRANSPORT_ROUTER_TAG, &UTRANSPORT_ROUTER_FN_START_TAG,
        );

        rx.recv().unwrap()?;

        Ok(UTransportRouterHandle {
            name: name.to_string(),
            command_sender,
            message_sender,
        })
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub(crate) struct ListenerMapKey {
    in_authority: UAuthority,
    out_authority: UAuthority,
    out_sender_wrapper: ComparableSender<UMessage>,
}

struct UTransportRouterInner {
    name: Arc<String>,
    utransport: Box<dyn UTransport>,
    listener_map: Arc<Mutex<HashMap<ListenerMapKey, String>>>,
    #[allow(dead_code)] // allow us flexibility in the future
    command_sender: Sender<UTransportRouterCommand>,
    #[allow(dead_code)] // allow us flexibility in the future
    command_receiver: Receiver<UTransportRouterCommand>,
    #[allow(dead_code)] // allow us flexibility in the future
    message_sender: ComparableSender<UMessage>,
    #[allow(dead_code)] // allow us flexibility in the future
    message_receiver: Receiver<UMessage>,
}

impl UTransportRouterInner {
    fn uauthority_to_uuri(authority: UAuthority) -> UUri {
        UUri {
            authority: Some(authority).into(),
            ..Default::default()
        }
    }

    pub async fn start<T>(
        name: String,
        utransport_builder: T,
        utransport_channels: UTransportChannels,
    ) -> Result<(), UStatus>
    where
        T: UTransportBuilder + 'static,
    {
        let name = name.clone();
        if log_enabled!(Level::Debug) {
            debug!(
                "{}:{}:{} Starting UTransportRouterInner with this configuration: ({:?})",
                &name,
                &UTRANSPORT_ROUTER_INNER_TAG,
                &UTRANSPORT_ROUTER_INNER_FN_START_TAG,
                utransport_builder.type_id(),
            );
        }

        let (tx, rx) = mpsc::channel::<Result<(), UStatus>>();

        thread::spawn(move || {
            if log_enabled!(Level::Debug) {
                debug!(
                    "{}:{}:{} Inside of thread::spawn()",
                    &name, &UTRANSPORT_ROUTER_INNER_TAG, &UTRANSPORT_ROUTER_INNER_FN_START_TAG,
                );
            }
            let mut result = Ok(());
            match utransport_builder.build() {
                Ok(utransport) => {
                    if log_enabled!(Level::Debug) {
                        debug!(
                            "{}:{}:{} Before creating UTransportRouterInner",
                            &name,
                            &UTRANSPORT_ROUTER_INNER_TAG,
                            &UTRANSPORT_ROUTER_INNER_FN_START_TAG,
                        );
                    }
                    let utransport_router_inner = Rc::new(UTransportRouterInner {
                        name: Arc::new(name.to_string()),
                        utransport,
                        listener_map: Arc::new(Mutex::new(HashMap::new())),
                        command_sender: utransport_channels.command_sender.clone(),
                        command_receiver: utransport_channels.command_receiver.clone(),
                        message_sender: utransport_channels.message_sender.clone(),
                        message_receiver: utransport_channels.message_receiver.clone(),
                    });

                    if log_enabled!(Level::Debug) {
                        debug!(
                            "{}:{}:{} After creating UTransportRouterInner",
                            &name,
                            &UTRANSPORT_ROUTER_INNER_TAG,
                            &UTRANSPORT_ROUTER_INNER_FN_START_TAG,
                        );
                    }

                    let utransport_router_inner_clone = utransport_router_inner.clone();
                    tx.send(result).unwrap();
                    task::block_on(async move {
                        if log_enabled!(Level::Debug) {
                            debug!(
                                "{}:{}:{} Calling into launch() which will block the newly spawned OS thread",
                                &name,
                                &UTRANSPORT_ROUTER_INNER_TAG,
                                &UTRANSPORT_ROUTER_INNER_FN_START_TAG,
                            );
                        }
                        utransport_router_inner_clone
                            .launch(
                                utransport_channels.command_receiver,
                                utransport_channels.message_receiver,
                            )
                            .await;
                    });
                }
                Err(status) => {
                    result = Err(UStatus::fail_with_code(
                        UCode::INTERNAL,
                        format!(
                            "{}: Failed to build Box<dyn UTransport> from UTransportBuilder: {status:?}",
                            &name
                        ),
                    ));
                    tx.send(result).unwrap();
                }
            }
        });
        rx.recv().unwrap()
    }

    async fn launch(
        &self,
        command_receiver: Receiver<UTransportRouterCommand>,
        message_receiver: Receiver<UMessage>,
    ) {
        let mut command_fut = command_receiver.recv().fuse();
        let mut message_fut = message_receiver.recv().fuse();

        if log_enabled!(Level::Debug) {
            debug!(
                "{}:{}:{} Reached launch()",
                &self.name, &UTRANSPORT_ROUTER_INNER_TAG, &UTRANSPORT_ROUTER_INNER_FN_LAUNCH_TAG,
            );
        }

        loop {
            if log_enabled!(Level::Debug) {
                debug!(
                    "{}:{}:{} Top of loop before select!",
                    &self.name,
                    &UTRANSPORT_ROUTER_INNER_TAG,
                    &UTRANSPORT_ROUTER_INNER_FN_LAUNCH_TAG,
                );
            }
            select! {
                command = command_fut => match command {
                    Ok(command) => {
                        if log_enabled!(Level::Debug) {
                            debug!(
                                "{}:{}:{} Received command: ({:?})",
                                &self.name,
                                &UTRANSPORT_ROUTER_INNER_TAG,
                                &UTRANSPORT_ROUTER_INNER_FN_LAUNCH_TAG,
                                &command
                            );
                        }
                        self.handle_command(command).await;
                        command_fut = command_receiver.recv().fuse(); // Re-arm future for the next iteration
                    },
                    Err(e) => {
                        error!(
                            "{}:{}:{} Unable to receive command: error: ({:?})",
                            &self.name,
                            &UTRANSPORT_ROUTER_INNER_TAG,
                            &UTRANSPORT_ROUTER_INNER_FN_LAUNCH_TAG,
                            e
                        );
                    },
                },
                message = message_fut => match message {
                    Ok(msg) => {
                        if log_enabled!(Level::Debug) {
                            debug!(
                                "{}:{}:{} Received a message intended to be sent out over our UTransport: message: {:?}",
                                &self.name,
                                &UTRANSPORT_ROUTER_INNER_TAG,
                                &UTRANSPORT_ROUTER_INNER_FN_LAUNCH_TAG,
                                &msg
                            );
                        }
                        self.send_over_utransport(msg).await;
                        message_fut = message_receiver.recv().fuse(); // Re-arm future for the next iteration
                    },
                    Err(e) => {
                        error!(
                            "{}:{}:{} Unable to receive message: error: ({:?})",
                            &self.name,
                            &UTRANSPORT_ROUTER_INNER_TAG,
                            &UTRANSPORT_ROUTER_INNER_FN_LAUNCH_TAG,
                            e
                        );
                    },
                },
            }
            if log_enabled!(Level::Debug) {
                debug!(
                    "{}:{}:{} Bottom of loop",
                    &self.name,
                    &UTRANSPORT_ROUTER_INNER_TAG,
                    &UTRANSPORT_ROUTER_INNER_FN_LAUNCH_TAG,
                );
            }
        }
    }

    async fn handle_command(&self, command: UTransportRouterCommand) {
        match command {
            UTransportRouterCommand::Register(register_control) => {
                let RegisterUnregisterControl {
                    in_authority,
                    out_authority,
                    out_sender_wrapper,
                    result_sender,
                } = register_control;

                if log_enabled!(Level::Debug) {
                    debug!(
                        "{}:{}:{} Received registration request for: ({:?}, {:?}, {})",
                        &self.name,
                        &UTRANSPORT_ROUTER_INNER_TAG,
                        &UTRANSPORT_ROUTER_INNER_FN_HANDLE_COMMAND_TAG,
                        &in_authority,
                        &out_authority,
                        &out_sender_wrapper.id
                    );
                }

                if self.message_sender == out_sender_wrapper {
                    let result_send_res = result_sender
                        .send(Err(UStatus::fail_with_code(
                            UCode::INVALID_ARGUMENT,
                            "Cannot send message to self!",
                        )))
                        .await;
                    if let Err(e) = result_send_res {
                        error!(
                            "{}:{}:{} Unable to return result: ({:?}, {:?}, {}), error: {:?}",
                            &self.name,
                            &UTRANSPORT_ROUTER_INNER_TAG,
                            &UTRANSPORT_ROUTER_INNER_FN_HANDLE_COMMAND_TAG,
                            &in_authority,
                            &out_authority,
                            &out_sender_wrapper.id,
                            e,
                        );
                    }
                    return;
                }

                let mut listener_map = self.listener_map.lock().await;

                let lister_map_key = ListenerMapKey {
                    in_authority: in_authority.clone(),
                    out_authority: out_authority.clone(),
                    out_sender_wrapper: out_sender_wrapper.clone(),
                };

                if listener_map.get(&lister_map_key.clone()).is_some() {
                    let result_send_res = result_sender
                        .send(Err(UStatus::fail_with_code(
                            UCode::ALREADY_EXISTS,
                            "Already registered!",
                        )))
                        .await;
                    if let Err(e) = result_send_res {
                        error!(
                            "{}:{}:{} Unable to return result: ({:?}, {:?}, {}), error: {:?}",
                            &self.name,
                            &UTRANSPORT_ROUTER_INNER_TAG,
                            &UTRANSPORT_ROUTER_INNER_FN_HANDLE_COMMAND_TAG,
                            &in_authority,
                            &out_authority,
                            &out_sender_wrapper.id,
                            e,
                        );
                    }
                    return;
                }

                let closure_name = self.name.clone();
                if listener_map.get(&lister_map_key.clone()).is_none() {
                    let out_sender_wrapper_closure = out_sender_wrapper.clone();
                    let callback_closure: Box<
                        dyn Fn(Result<UMessage, UStatus>) + Send + Sync + 'static,
                    > = Box::new(move |received: Result<UMessage, UStatus>| {
                        let out_sender_wrapper_closure = out_sender_wrapper_closure.clone();
                        task::spawn_local(forwarding_callback(
                            closure_name.clone(),
                            received,
                            out_sender_wrapper_closure.clone(),
                        ));
                    });

                    let registration_uuri =
                        UTransportRouterInner::uauthority_to_uuri(in_authority.clone());
                    let registration_result = self
                        .utransport
                        .register_listener(registration_uuri, callback_closure)
                        .await;
                    if let Ok(registration_string) = registration_result {
                        listener_map.insert(lister_map_key, registration_string);
                    }
                }

                let result_send_res = result_sender.send(Ok(())).await;
                if let Err(e) = result_send_res {
                    error!(
                        "{}:{}:{} Unable to return result: ({:?}, {:?}, {}), error: {:?}",
                        &self.name,
                        &UTRANSPORT_ROUTER_INNER_TAG,
                        &UTRANSPORT_ROUTER_INNER_FN_HANDLE_COMMAND_TAG,
                        &in_authority,
                        &out_authority,
                        &out_sender_wrapper.id,
                        e,
                    );
                }
            }
            UTransportRouterCommand::Unregister(unregister_control) => {
                let RegisterUnregisterControl {
                    in_authority,
                    out_authority,
                    out_sender_wrapper,
                    result_sender,
                } = unregister_control;

                if log_enabled!(Level::Debug) {
                    debug!(
                        "{}:{}:{} Received unregistration request for: ({:?}, {:?}, {})",
                        &self.name,
                        &UTRANSPORT_ROUTER_INNER_TAG,
                        &UTRANSPORT_ROUTER_INNER_FN_HANDLE_COMMAND_TAG,
                        &in_authority,
                        &out_authority,
                        &out_sender_wrapper.id
                    );
                }

                if self.message_sender == out_sender_wrapper {
                    let result_send_res = result_sender
                        .send(Err(UStatus::fail_with_code(
                            UCode::INVALID_ARGUMENT,
                            "Cannot send message to self!",
                        )))
                        .await;
                    if let Err(e) = result_send_res {
                        error!(
                            "{}:{}:{} Unable to return result: ({:?}, {:?}, {}), error: {:?}",
                            &self.name,
                            &UTRANSPORT_ROUTER_INNER_TAG,
                            &UTRANSPORT_ROUTER_INNER_FN_HANDLE_COMMAND_TAG,
                            &in_authority,
                            &out_authority,
                            &out_sender_wrapper.id,
                            e,
                        );
                    }
                    return;
                }

                let mut listener_map = self.listener_map.lock().await;

                let lister_map_key = ListenerMapKey {
                    in_authority: in_authority.clone(),
                    out_authority: out_authority.clone(),
                    out_sender_wrapper: out_sender_wrapper.clone(),
                };

                if listener_map.remove(&lister_map_key).is_none() {
                    let result_send_res = result_sender
                        .send(Err(UStatus::fail_with_code(
                            UCode::NOT_FOUND,
                            "Cannot find this one to remove!",
                        )))
                        .await;
                    if let Err(e) = result_send_res {
                        error!(
                            "{}:{}:{} Unable to return result: ({:?}, {:?}, {}), error: {:?}",
                            &self.name,
                            &UTRANSPORT_ROUTER_INNER_TAG,
                            &UTRANSPORT_ROUTER_INNER_FN_HANDLE_COMMAND_TAG,
                            &in_authority,
                            &out_authority,
                            &out_sender_wrapper.id,
                            e,
                        );
                    }
                    return;
                }

                let result_send_res = result_sender.send(Ok(())).await;
                if let Err(e) = result_send_res {
                    error!(
                        "{}:{}:{} Unable to return result: ({:?}, {:?}, {}), error: {:?}",
                        &self.name,
                        &UTRANSPORT_ROUTER_INNER_TAG,
                        &UTRANSPORT_ROUTER_INNER_FN_HANDLE_COMMAND_TAG,
                        &in_authority,
                        &out_authority,
                        &out_sender_wrapper.id,
                        e,
                    );
                }
            }
        }
    }

    async fn send_over_utransport(&self, message: UMessage) {
        if log_enabled!(Level::Debug) {
            debug!(
                "{}:{}:{} Sending message over UTransport: {:?}",
                &self.name,
                &UTRANSPORT_ROUTER_INNER_TAG,
                &UTRANSPORT_ROUTER_INNER_FN_SEND_OVER_TRANSPORT_TAG,
                &message.clone(),
            );
        }
        let send_result = self.utransport.send(message).await;
        // unfortunately because send() takes ownership of message, it would be required to clone()
        // before calling send(). However, that feels wasteful to do for some small percentage of
        // times that send() doesn't succeed
        // that's why for now we don't include the message itself in the error!()
        if let Err(e) = send_result {
            error!(
                "{}:{}:{} Failed to send message over UTransport: error: {:?}",
                &self.name,
                &UTRANSPORT_ROUTER_INNER_TAG,
                &UTRANSPORT_ROUTER_INNER_FN_SEND_OVER_TRANSPORT_TAG,
                e
            );
        }
    }
}

async fn forwarding_callback(
    name: Arc<String>,
    received: Result<UMessage, UStatus>,
    out_sender_wrapper: ComparableSender<UMessage>,
) {
    if log_enabled!(Level::Debug) {
        debug!(
            "{}:{}:{} Forwarding message from this UTransportRouter onto another UTransportRouter's Receiver<UMessage>: {}",
            &name,
            &UTRANSPORT_ROUTER_INNER_TAG,
            &UTRANSPORT_ROUTER_INNER_FN_FORWARDING_CALLBACK_TAG,
            &out_sender_wrapper.id
        );
    }
    if let Ok(msg) = received {
        let forward_result = out_sender_wrapper.send(msg).await;
        if let Err(e) = forward_result {
            debug!(
                "{}:{}:{} Forwarding message from this UTransportRouter onto another UTransportRouter's Receiver<UMessage> failed: {}; error: {:?}",
                &name,
                &UTRANSPORT_ROUTER_INNER_TAG,
                &UTRANSPORT_ROUTER_INNER_FN_FORWARDING_CALLBACK_TAG,
                &out_sender_wrapper.id,
                e
            );
        }
    }
}

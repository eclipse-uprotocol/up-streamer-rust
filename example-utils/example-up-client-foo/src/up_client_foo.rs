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

use async_broadcast::{Receiver, Sender};
use async_std::sync::Mutex;
use async_std::task;
use async_trait::async_trait;
use log::debug;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use up_rust::{
    ComparableListener, UAuthority, UCode, UListener, UMessage, UMessageType, UStatus, UTransport,
    UUri,
};

pub struct UPClientFoo {
    #[allow(dead_code)]
    name: Arc<String>,
    #[allow(dead_code)]
    protocol_receiver: Receiver<Result<UMessage, UStatus>>,
    protocol_sender: Sender<Result<UMessage, UStatus>>,
    listeners: Arc<Mutex<HashMap<UUri, HashSet<ComparableListener>>>>,
    authority_listeners: Arc<Mutex<HashMap<UAuthority, HashSet<ComparableListener>>>>,
    pub times_received: Arc<AtomicU64>,
}

impl UPClientFoo {
    pub async fn new(
        name: &str,
        protocol_receiver: Receiver<Result<UMessage, UStatus>>,
        protocol_sender: Sender<Result<UMessage, UStatus>>,
    ) -> Self {
        let name = Arc::new(name.to_string());
        let listeners: Arc<Mutex<HashMap<UUri, HashSet<ComparableListener>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let authority_listeners: Arc<Mutex<HashMap<UAuthority, HashSet<ComparableListener>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let name_clone = name.clone();
        let authority_listeners_clone = authority_listeners.clone();
        let listeners_clone = listeners.clone();
        let protocol_receiver_clone = protocol_receiver.clone();

        let times_received = Arc::new(AtomicU64::new(0));
        let times_received_task = times_received.clone();
        task::spawn(async move {
            let name_clone = name_clone.clone();
            let mut protocol_receiver_clone = protocol_receiver_clone.clone();
            let listeners_clone = listeners_clone.clone();

            while let Ok(received) = protocol_receiver_clone.recv().await {
                match &received {
                    Ok(msg) => {
                        let UMessage { attributes, .. } = &msg;
                        let Some(attr) = attributes.as_ref() else {
                            debug!("{}: No UAttributes!", &name_clone);
                            continue;
                        };

                        match attr.type_.enum_value() {
                            Ok(enum_value) => match enum_value {
                                UMessageType::UMESSAGE_TYPE_UNSPECIFIED => {
                                    debug!("{}: Type unspecified! Fail!", &name_clone);
                                }
                                UMessageType::UMESSAGE_TYPE_NOTIFICATION => {
                                    let sink_uuri = attr.sink.as_ref();
                                    debug!("{}: Request sink uuri: {sink_uuri:?}", &name_clone);
                                    match sink_uuri {
                                        None => {
                                            debug!("{}: No sink uuri!", &name_clone);
                                        }
                                        Some(topic) => {
                                            let authority_listeners =
                                                authority_listeners_clone.lock().await;
                                            if let Some(authority) = topic.authority.as_ref() {
                                                debug!(
                                                    "{}: Notification: authority: {authority:?}",
                                                    &name_clone
                                                );

                                                let authority_listeners =
                                                    authority_listeners.get(authority);

                                                if let Some(authority_listeners) =
                                                    authority_listeners
                                                {
                                                    debug!(
                                                        "{}: Notification: authority: {authority:?} -- listeners found",
                                                        &name_clone
                                                    );

                                                    for al in authority_listeners.iter() {
                                                        al.on_receive(msg.clone()).await;
                                                    }
                                                } else {
                                                    debug!(
                                                        "{}: Notification: authority: {authority:?} -- no listeners",
                                                        &name_clone
                                                    );
                                                }
                                            }

                                            let listeners = listeners_clone.lock().await;
                                            let topic_listeners = listeners.get(topic);

                                            if let Some(topic_listeners) = topic_listeners {
                                                debug!(
                                                    "{}: Notification: topic: {topic:?} -- listeners found",
                                                    &name_clone
                                                );
                                                times_received_task.fetch_add(1, Ordering::Relaxed);
                                                for tl in topic_listeners.iter() {
                                                    tl.on_receive(msg.clone()).await;
                                                }
                                            } else {
                                                debug!(
                                                    "{}: Notification: topic: {topic:?} -- listeners not found",
                                                    &name_clone
                                                );
                                            }
                                        }
                                    }
                                }
                                UMessageType::UMESSAGE_TYPE_PUBLISH => {
                                    unimplemented!("Still need to handle Publish messages");
                                }
                                UMessageType::UMESSAGE_TYPE_REQUEST => {
                                    let sink_uuri = attr.sink.as_ref();
                                    debug!("{}: Request sink uuri: {sink_uuri:?}", &name_clone);
                                    match sink_uuri {
                                        None => {
                                            debug!("{}: No sink uuri!", &name_clone);
                                        }
                                        Some(topic) => {
                                            let authority_listeners =
                                                authority_listeners_clone.lock().await;
                                            if let Some(authority) = topic.authority.as_ref() {
                                                debug!(
                                                    "{}: Request: authority: {authority:?}",
                                                    &name_clone
                                                );

                                                let authority_listeners =
                                                    authority_listeners.get(authority);

                                                if let Some(authority_listeners) =
                                                    authority_listeners
                                                {
                                                    debug!(
                                                        "{}: Request: authority: {authority:?} -- listeners found",
                                                        &name_clone
                                                    );
                                                    for al in authority_listeners.iter() {
                                                        al.on_receive(msg.clone()).await;
                                                    }
                                                } else {
                                                    debug!(
                                                        "{}: Request: authority: {authority:?} -- no listeners",
                                                        &name_clone
                                                    );
                                                }
                                            }

                                            let listeners = listeners_clone.lock().await;
                                            let topic_listeners = listeners.get(topic);

                                            if let Some(topic_listeners) = topic_listeners {
                                                debug!(
                                                    "{}: Request: topic: {topic:?} -- listeners found",
                                                    &name_clone
                                                );
                                                times_received_task.fetch_add(1, Ordering::Relaxed);
                                                for tl in topic_listeners.iter() {
                                                    tl.on_receive(msg.clone()).await;
                                                }
                                            } else {
                                                debug!(
                                                    "{}: Request: topic: {topic:?} -- listeners not found",
                                                    &name_clone
                                                );
                                            }
                                        }
                                    }
                                }
                                UMessageType::UMESSAGE_TYPE_RESPONSE => {
                                    let sink_uuri = attr.sink.as_ref();
                                    debug!("{}: Response sink uuri: {sink_uuri:?}", &name_clone);
                                    match sink_uuri {
                                        None => {
                                            debug!("{}: No sink uuri!", &name_clone);
                                        }
                                        Some(topic) => {
                                            let authority_listeners =
                                                authority_listeners_clone.lock().await;
                                            if let Some(authority) = topic.authority.as_ref() {
                                                debug!(
                                                    "{}: Response: authority: {authority:?}",
                                                    &name_clone
                                                );

                                                let authority_listeners =
                                                    authority_listeners.get(authority);

                                                if let Some(authority_listeners) =
                                                    authority_listeners
                                                {
                                                    debug!(
                                                        "{}: Response: authority: {authority:?} -- listeners found",
                                                        &name_clone
                                                    );
                                                    for al in authority_listeners.iter() {
                                                        al.on_receive(msg.clone()).await;
                                                    }
                                                } else {
                                                    debug!(
                                                        "{}: Response: authority: {authority:?} -- no listeners",
                                                        &name_clone
                                                    );
                                                }
                                            }

                                            let listeners = listeners_clone.lock().await;
                                            let topic_listeners = listeners.get(topic);

                                            if let Some(topic_listeners) = topic_listeners {
                                                debug!(
                                                    "{}: Response: topic: {topic:?} -- listeners found",
                                                    &name_clone
                                                );
                                                times_received_task.fetch_add(1, Ordering::Relaxed);
                                                for tl in topic_listeners.iter() {
                                                    tl.on_receive(msg.clone()).await;
                                                }
                                            } else {
                                                debug!(
                                                    "{}: Response: topic: {topic:?} -- listeners not found",
                                                    &name_clone
                                                );
                                            }
                                        }
                                    }
                                }
                            },
                            Err(_) => {
                                debug!("No matching type or an error occurred!");
                            }
                        }
                    }
                    Err(status) => {
                        debug!("Got an error! err: {status:?}");
                    }
                }
            }
        });

        Self {
            name,
            protocol_sender,
            protocol_receiver,
            listeners,
            authority_listeners,
            times_received,
        }
    }
}

#[async_trait]
impl UTransport for UPClientFoo {
    async fn send(&self, message: UMessage) -> Result<(), UStatus> {
        debug!("sending: {message:?}");
        match self.protocol_sender.broadcast(Ok(message)).await {
            Ok(_) => Ok(()),
            Err(_) => Err(UStatus::fail_with_code(
                UCode::INTERNAL,
                "Unable to send over Foo protocol",
            )),
        }
    }

    async fn receive(&self, _topic: UUri) -> Result<UMessage, UStatus> {
        unimplemented!()
    }

    async fn register_listener(
        &self,
        topic: UUri,
        listener: Arc<dyn UListener>,
    ) -> Result<(), UStatus> {
        debug!("{}: registering listener for: {topic:?}", &self.name);

        return if topic.resource.is_none() && topic.entity.is_none() {
            debug!("{}: registering authority listener", &self.name);

            let mut authority_listeners = self.authority_listeners.lock().await;
            let Some(authority) = topic.authority.as_ref() else {
                return Err(UStatus::fail_with_code(
                    UCode::INVALID_ARGUMENT,
                    "No authority provided!",
                ));
            };
            let authority_listeners = authority_listeners.entry(authority.clone()).or_default();
            let comparable_listener = ComparableListener::new(listener);
            let inserted = authority_listeners.insert(comparable_listener);

            match inserted {
                true => Ok(()),
                false => Err(UStatus::fail_with_code(
                    UCode::ALREADY_EXISTS,
                    "UUri and listener already registered!",
                )),
            }
        } else {
            debug!("{}: registering regular listener", &self.name);

            let mut listeners = self.listeners.lock().await;
            let topic_listeners = listeners.entry(topic).or_default();
            let comparable_listener = ComparableListener::new(listener);
            let inserted = topic_listeners.insert(comparable_listener);

            match inserted {
                true => Ok(()),
                false => Err(UStatus::fail_with_code(
                    UCode::ALREADY_EXISTS,
                    "UUri and listener already registered!",
                )),
            }
        };
    }

    async fn unregister_listener(
        &self,
        topic: UUri,
        listener: Arc<dyn UListener>,
    ) -> Result<(), UStatus> {
        let mut listeners = self.listeners.lock().await;
        let Some(topic_listeners) = listeners.get_mut(&topic) else {
            return Err(UStatus::fail_with_code(
                UCode::NOT_FOUND,
                "No listeners registered for topic!",
            ));
        };
        let comparable_listener = ComparableListener::new(listener);
        let removed = topic_listeners.remove(&comparable_listener);

        return match removed {
            false => Err(UStatus::fail_with_code(
                UCode::NOT_FOUND,
                "No listeners registered for topic!",
            )),
            true => Ok(()),
        };
    }
}

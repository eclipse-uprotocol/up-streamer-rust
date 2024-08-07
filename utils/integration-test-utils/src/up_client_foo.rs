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
use log::{debug, error};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use up_rust::{
    ComparableListener, UAttributes, UCode, UListener, UMessage, UMessageType, UStatus, UTransport,
    UUri,
};

type TopicListenerMap = Arc<Mutex<HashMap<(UUri, Option<UUri>), HashSet<ComparableListener>>>>;
type AuthorityListenerMap = Arc<Mutex<HashMap<String, HashSet<ComparableListener>>>>;

pub struct UPClientFoo {
    name: Arc<String>,
    protocol_receiver: Receiver<Result<UMessage, UStatus>>,
    protocol_sender: Sender<Result<UMessage, UStatus>>,
    listeners: TopicListenerMap,
    authority_listeners: AuthorityListenerMap,
    pub times_received: Arc<AtomicU64>,
}

impl UPClientFoo {
    pub async fn new(
        name: &str,
        protocol_receiver: Receiver<Result<UMessage, UStatus>>,
        protocol_sender: Sender<Result<UMessage, UStatus>>,
    ) -> Self {
        let name = Arc::new(name.to_string());
        let listeners = Arc::new(Mutex::new(HashMap::new()));
        let authority_listeners = Arc::new(Mutex::new(HashMap::new()));

        let times_received = Arc::new(AtomicU64::new(0));

        let me = Self {
            name,
            protocol_sender,
            protocol_receiver,
            listeners,
            authority_listeners,
            times_received,
        };

        me.listen_loop().await;

        me
    }

    async fn listen_loop(&self) {
        let name = self.name.clone();
        let mut protocol_receiver = self.protocol_receiver.clone();
        let listeners = self.listeners.clone();
        let authority_listeners = self.authority_listeners.clone();
        let times_received = self.times_received.clone();
        thread::spawn(move || {
            task::block_on(async {
                while let Ok(received) = protocol_receiver.recv().await {
                    match &received {
                        Ok(msg) => {
                            let UMessage { attributes, .. } = &msg;
                            let Some(attr) = attributes.as_ref() else {
                                debug!("{}: No UAttributes!", &name);
                                continue;
                            };

                            match attr.type_.enum_value().unwrap_or_default() {
                                UMessageType::UMESSAGE_TYPE_NOTIFICATION => {
                                    UPClientFoo::process_message(
                                        &name,
                                        msg,
                                        attr,
                                        "Notification",
                                        listeners.clone(),
                                        authority_listeners.clone(),
                                        times_received.clone(),
                                    )
                                    .await;
                                }
                                UMessageType::UMESSAGE_TYPE_PUBLISH => {
                                    unimplemented!("Still need to handle Publish messages");
                                }
                                UMessageType::UMESSAGE_TYPE_REQUEST => {
                                    UPClientFoo::process_message(
                                        &name,
                                        msg,
                                        attr,
                                        "Request",
                                        listeners.clone(),
                                        authority_listeners.clone(),
                                        times_received.clone(),
                                    )
                                    .await;
                                }
                                UMessageType::UMESSAGE_TYPE_RESPONSE => {
                                    UPClientFoo::process_message(
                                        &name,
                                        msg,
                                        attr,
                                        "Response",
                                        listeners.clone(),
                                        authority_listeners.clone(),
                                        times_received.clone(),
                                    )
                                    .await;
                                }
                                _ => {
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
        });
    }

    async fn process_message(
        name: &str,
        msg: &UMessage,
        attr: &UAttributes,
        msg_type: &str,
        listeners: TopicListenerMap,
        authority_listeners: AuthorityListenerMap,
        times_received: Arc<AtomicU64>,
    ) {
        let sink_uuri = attr.sink.as_ref();
        debug!("{}: {msg_type} sink uuri: {sink_uuri:?}", name);
        match sink_uuri {
            None => {
                debug!("{}: No source uuri!", name);
            }
            Some(sink) => {
                let authority_name = sink.authority_name.clone();
                let authority_listeners = authority_listeners.lock().await;
                debug!("{}: {msg_type}: authority_name: {authority_name}", name);

                let authority_listeners = authority_listeners.get(&authority_name);
                if let Some(authority_listeners) = authority_listeners {
                    debug!(
                        "{}: {msg_type}: authority listeners found: {authority_name:?}",
                        name
                    );

                    for (authority_listener_num, al) in authority_listeners.iter().enumerate() {
                        debug!(
                            "{}: {msg_type}: Authority listener num: {}",
                            name, authority_listener_num
                        );
                        al.on_receive(msg.clone()).await;
                    }
                } else {
                    debug!(
                        "{}: {msg_type}: authority no listeners: {authority_name:?}",
                        name
                    );
                }

                let listeners = listeners.lock().await;
                let topic_listeners = listeners.get(&(
                    attr.source.as_ref().cloned().unwrap(),
                    attr.sink.as_ref().cloned(),
                ));

                if let Some(topic_listeners) = topic_listeners {
                    debug!(
                        "{}: {msg_type}: source: {:?} sink: {:?} -- topic listeners found",
                        name,
                        attr.source.as_ref(),
                        attr.sink.as_ref()
                    );
                    times_received.fetch_add(1, Ordering::SeqCst);
                    for tl in topic_listeners.iter() {
                        tl.on_receive(msg.clone()).await;
                    }
                } else {
                    debug!(
                        "{}: {msg_type}: source: {:?} sink: {:?} -- listeners not found",
                        name,
                        attr.source.as_ref(),
                        attr.sink.as_ref()
                    );
                }
            }
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

    async fn receive(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
    ) -> Result<UMessage, UStatus> {
        unimplemented!()
    }

    async fn register_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UListener>,
    ) -> Result<(), UStatus> {
        debug!(
            "{}: registering listener for: source: {:?} sink: {:?}",
            self.name, source_filter, sink_filter
        );

        let sink_for_specific = {
            if let Some(sink) = sink_filter {
                sink.authority_name != "*"
            } else {
                false
            }
        };

        return if source_filter.authority_name == "*" && sink_for_specific {
            let sink_authority = sink_filter.unwrap().clone().authority_name;
            let mut authority_listeners = self.authority_listeners.lock().await;
            let authority = sink_authority;
            debug!(
                "{}: registering authority listener on authority: {}",
                &self.name, authority
            );
            let authority_listeners = authority_listeners.entry(authority.clone()).or_default();
            let comparable_listener = ComparableListener::new(listener);
            let inserted = authority_listeners.insert(comparable_listener);

            match inserted {
                true => {
                    debug!("{}: successfully registered authority listener for: authority: {}", &self.name, authority);

                    Ok(())
                },
                false => Err(UStatus::fail_with_code(
                    UCode::ALREADY_EXISTS,
                    format!("{}: UUri and listener already registered! failed to register authority listener for: authority: {}", &self.name, authority)
                )),
            }
        } else {
            debug!(
                "{}: registering regular listener for: source: {:?} sink: {:?}",
                &self.name, source_filter, sink_filter
            );

            let mut listeners = self.listeners.lock().await;
            let topic_listeners = listeners
                .entry((source_filter.clone(), sink_filter.cloned()))
                .or_default();
            let comparable_listener = ComparableListener::new(listener);
            let inserted = topic_listeners.insert(comparable_listener);

            match inserted {
                true => {
                    debug!(
                        "{}: successfully registered regular listener for: source: {:?} sink: {:?}",
                        &self.name, source_filter, sink_filter
                    );

                    Ok(())
                }
                false => Err(UStatus::fail_with_code(
                    UCode::ALREADY_EXISTS,
                    "UUri and listener already registered!",
                )),
            }
        };
    }

    async fn unregister_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UListener>,
    ) -> Result<(), UStatus> {
        debug!(
            "{} unregistering listener for source_filter: {source_filter:?}",
            &self.name
        );

        let sink_for_any = {
            if let Some(sink) = sink_filter {
                sink.authority_name == "*"
            } else {
                false
            }
        };

        return if source_filter.authority_name != "*" && sink_for_any {
            debug!("{}: unregistering authority listener", &self.name);

            let mut authority_listeners = self.authority_listeners.lock().await;

            let authority = source_filter.authority_name.clone();

            let Some(authority_listeners) = authority_listeners.get_mut(&authority) else {
                let err = UStatus::fail_with_code(
                    UCode::NOT_FOUND,
                    format!("{} No authority listeners for: source: {:?} sink: {:?} -- unable to unregister", self.name, source_filter, sink_filter)
                );
                error!("{} {err:?}", &self.name);
                return Err(err);
            };

            let comparable_listener = ComparableListener::new(listener);
            let removed = authority_listeners.remove(&comparable_listener);
            match removed {
                true => Ok(()),
                false => {
                    let err = UStatus::fail_with_code(
                        UCode::NOT_FOUND,
                        format!("{} Unable to find authority listener for: source: {:?} sink: {:?} -- unable to unregister", self.name, source_filter, sink_filter)
                    );
                    error!("{} {err:?}", &self.name);
                    Err(err)
                }
            }
        } else {
            let mut listeners = self.listeners.lock().await;
            let Some(topic_listeners) =
                listeners.get_mut(&(source_filter.clone(), sink_filter.cloned()))
            else {
                return Err(UStatus::fail_with_code(
                    UCode::NOT_FOUND,
                    "No listeners registered for topic!",
                ));
            };
            let comparable_listener = ComparableListener::new(listener);
            let removed = topic_listeners.remove(&comparable_listener);

            match removed {
                false => Err(UStatus::fail_with_code(
                    UCode::NOT_FOUND,
                    "No listeners registered for topic! topic: {topic:?}",
                )),
                true => Ok(()),
            }
        };
    }
}

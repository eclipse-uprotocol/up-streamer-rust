use crate::UTransportListener;
use async_broadcast::{Receiver, Sender};
use async_std::sync::Mutex;
use async_std::task;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use up_rust::{UAuthority, UCode, UMessage, UMessageType, UStatus, UTransport, UUIDBuilder, UUri};
use up_streamer::UTransportBuilder;

pub struct UPClientFoo {
    #[allow(dead_code)]
    name: Arc<String>,
    #[allow(dead_code)]
    protocol_receiver: Receiver<Result<UMessage, UStatus>>,
    protocol_sender: Sender<Result<UMessage, UStatus>>,
    uuid_builder: UUIDBuilder,
    listeners: Arc<Mutex<HashMap<UUri, HashMap<String, UTransportListener>>>>,
    authority_listeners: Arc<Mutex<HashMap<UAuthority, HashMap<String, UTransportListener>>>>,
}

impl UPClientFoo {
    pub async fn new(
        name: &str,
        protocol_receiver: Receiver<Result<UMessage, UStatus>>,
        protocol_sender: Sender<Result<UMessage, UStatus>>,
    ) -> Self {
        let name = Arc::new(name.to_string());
        let listeners: Arc<Mutex<HashMap<UUri, HashMap<String, UTransportListener>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let authority_listeners: Arc<
            Mutex<HashMap<UAuthority, HashMap<String, UTransportListener>>>,
        > = Arc::new(Mutex::new(HashMap::new()));

        let name_clone = name.clone();
        let authority_listeners_clone = authority_listeners.clone();
        let listeners_clone = listeners.clone();
        let protocol_receiver_clone = protocol_receiver.clone();
        task::spawn(async move {
            let name_clone = name_clone.clone();
            let mut protocol_receiver_clone = protocol_receiver_clone.clone();
            let listeners_clone = listeners_clone.clone();

            while let Ok(received) = protocol_receiver_clone.recv().await {
                match &received {
                    Ok(msg) => {
                        let UMessage { attributes, .. } = &msg;
                        let Some(attr) = attributes.as_ref() else {
                            println!("{}: No UAttributes!", &name_clone);
                            continue;
                        };

                        match attr.type_.enum_value() {
                            Ok(enum_value) => match enum_value {
                                UMessageType::UMESSAGE_TYPE_UNSPECIFIED => {
                                    println!("{}: Type unspecified! Fail!", &name_clone);
                                }
                                UMessageType::UMESSAGE_TYPE_PUBLISH => {
                                    let src_uuri = attr.source.as_ref();
                                    match src_uuri {
                                        None => {
                                            println!("{}: No source uuri!", &name_clone);
                                        }
                                        Some(topic) => {
                                            let authority_listeners =
                                                authority_listeners_clone.lock().await;
                                            if let Some(authority) = topic.authority.as_ref() {
                                                let authority_listeners =
                                                    authority_listeners.get(authority);
                                                match authority_listeners {
                                                    None => {
                                                        println!("{}: Publish: No authority listeners for topic!: {:?}", &name_clone, &topic);
                                                    }
                                                    Some(authority_listeners) => {
                                                        println!("{}: Publish: Found authority listeners for topic!", &name_clone);
                                                        for al in authority_listeners.iter() {
                                                            println!("{}: sending out over registration_string: {}", &name_clone, al.0);
                                                            al.1(Ok(msg.clone()))
                                                        }
                                                    }
                                                }
                                            }

                                            let listeners = listeners_clone.lock().await;
                                            let topic_listeners = listeners.get(topic);
                                            match topic_listeners {
                                                None => {
                                                    println!(
                                                        "{}: Publish: No listeners for topic! message: {msg:?}",
                                                        &name_clone
                                                    );
                                                }
                                                Some(topic_listeners) => {
                                                    for tl in topic_listeners.iter() {
                                                        tl.1(Ok(msg.clone()))
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                UMessageType::UMESSAGE_TYPE_REQUEST => {
                                    let sink_uuri = attr.sink.as_ref();
                                    println!("{}: Request sink uuri: {sink_uuri:?}", &name_clone);
                                    match sink_uuri {
                                        None => {
                                            println!("{}: No sink uuri!", &name_clone);
                                        }
                                        Some(topic) => {
                                            let authority_listeners =
                                                authority_listeners_clone.lock().await;
                                            if let Some(authority) = topic.authority.as_ref() {
                                                println!(
                                                    "{}: Request: authority: {authority:?}",
                                                    &name_clone
                                                );

                                                let authority_listeners =
                                                    authority_listeners.get(authority);
                                                match authority_listeners {
                                                    None => {
                                                        println!("{}: Request: No authority listeners for topic! {:?}", &name_clone, &topic);
                                                    }
                                                    Some(authority_listeners) => {
                                                        println!("{}: Request: Found authority listeners for topic!", &name_clone);
                                                        for al in authority_listeners.iter() {
                                                            println!("{}: sending out over registration_string: {}", &name_clone, al.0);
                                                            al.1(Ok(msg.clone()))
                                                        }
                                                    }
                                                }
                                            }

                                            let listeners = listeners_clone.lock().await;
                                            let topic_listeners = listeners.get(topic);
                                            match topic_listeners {
                                                None => {
                                                    println!(
                                                        "{}: Request: No listeners for topic! message: {msg:?}",
                                                        &name_clone
                                                    );
                                                }
                                                Some(topic_listeners) => {
                                                    for tl in topic_listeners.iter() {
                                                        tl.1(Ok(msg.clone()))
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                UMessageType::UMESSAGE_TYPE_RESPONSE => {
                                    let sink_uuri = attr.sink.as_ref();
                                    println!("{}: Response sink uuri: {sink_uuri:?}", &name_clone);
                                    match sink_uuri {
                                        None => {
                                            println!("{}: No sink uuri!", &name_clone);
                                        }
                                        Some(topic) => {
                                            let authority_listeners =
                                                authority_listeners_clone.lock().await;
                                            if let Some(authority) = topic.authority.as_ref() {
                                                println!(
                                                    "{}: Response: authority: {authority:?}",
                                                    &name_clone
                                                );

                                                let authority_listeners =
                                                    authority_listeners.get(authority);
                                                match authority_listeners {
                                                    None => {
                                                        println!("{}: Response: No authority listeners for topic! {:?}", &name_clone, &topic);
                                                    }
                                                    Some(authority_listeners) => {
                                                        println!("{}: Response: Found authority listeners for topic!", &name_clone);
                                                        for al in authority_listeners.iter() {
                                                            println!("{}: sending out over registration_string: {}", &name_clone, al.0);
                                                            al.1(Ok(msg.clone()))
                                                        }
                                                    }
                                                }
                                            }

                                            let listeners = listeners_clone.lock().await;
                                            let topic_listeners = listeners.get(topic);
                                            match topic_listeners {
                                                None => {
                                                    println!(
                                                        "{}: Reponse: No listeners for topic! message: {msg:?}",
                                                        &name_clone
                                                    );
                                                }
                                                Some(topic_listeners) => {
                                                    for tl in topic_listeners.iter() {
                                                        tl.1(Ok(msg.clone()))
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                            Err(_) => {
                                println!("No matching type or an error occurred!");
                            }
                        }
                    }
                    Err(status) => {
                        println!("Got an error! err: {status:?}");
                    }
                }
            }
        });

        Self {
            name,
            protocol_sender,
            protocol_receiver,
            uuid_builder: UUIDBuilder::new(),
            listeners,
            authority_listeners,
        }
    }
}

#[async_trait]
impl UTransport for UPClientFoo {
    async fn send(&self, message: UMessage) -> Result<(), UStatus> {
        println!("sending: {message:?}");
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
        listener: Box<dyn Fn(Result<UMessage, UStatus>) + Send + Sync + 'static>,
    ) -> Result<String, UStatus> {
        println!("{}: registering listener for: {topic:?}", &self.name);

        return if topic.resource.is_none() && topic.entity.is_none() {
            println!("{}: registering authority listener", &self.name);

            let mut authority_listeners = self.authority_listeners.lock().await;
            let Some(authority) = topic.authority.as_ref() else {
                return Err(UStatus::fail_with_code(
                    UCode::INVALID_ARGUMENT,
                    "No authority provided!",
                ));
            };
            let authority_listeners = authority_listeners.entry(authority.clone()).or_default();
            let registration_string = self.uuid_builder.build().to_string();
            let inserted = authority_listeners.insert(registration_string.clone(), listener);

            match inserted {
                None => Ok(registration_string),
                Some(_) => Err(UStatus::fail_with_code(
                    UCode::ALREADY_EXISTS,
                    "UUri and listener already registered!",
                )),
            }
        } else {
            println!("{}: registering regular listener", &self.name);

            let mut listeners = self.listeners.lock().await;
            let topic_listeners = listeners.entry(topic).or_default();
            let registration_string = self.uuid_builder.build().to_string();
            let inserted = topic_listeners.insert(registration_string.clone(), listener);

            match inserted {
                None => Ok(registration_string),
                Some(_) => Err(UStatus::fail_with_code(
                    UCode::ALREADY_EXISTS,
                    "UUri and listener already registered!",
                )),
            }
        };
    }

    async fn unregister_listener(&self, topic: UUri, listener: &str) -> Result<(), UStatus> {
        let mut listeners = self.listeners.lock().await;
        let Some(topic_listeners) = listeners.get_mut(&topic) else {
            return Err(UStatus::fail_with_code(
                UCode::NOT_FOUND,
                "No listeners registered for topic!",
            ));
        };
        let removed = topic_listeners.remove(listener);

        return match removed {
            None => Err(UStatus::fail_with_code(
                UCode::NOT_FOUND,
                "No listeners registered for topic!",
            )),
            Some(_) => Ok(()),
        };
    }
}

pub struct UTransportBuilderFoo {
    name: Arc<String>,
    protocol_receiver: Receiver<Result<UMessage, UStatus>>,
    protocol_sender: Sender<Result<UMessage, UStatus>>,
}

impl UTransportBuilderFoo {
    pub fn new(
        name: &str,
        protocol_receiver: Receiver<Result<UMessage, UStatus>>,
        protocol_sender: Sender<Result<UMessage, UStatus>>,
    ) -> Self {
        let name = Arc::new(name.to_string());
        Self {
            name,
            protocol_receiver,
            protocol_sender,
        }
    }
}

impl UTransportBuilder for UTransportBuilderFoo {
    fn build(&self) -> Result<Box<dyn UTransport>, UStatus> {
        let utransport: Box<dyn UTransport> = Box::new(task::block_on(UPClientFoo::new(
            &self.name,
            self.protocol_receiver.clone(),
            self.protocol_sender.clone(),
        )));
        Ok(utransport)
    }
}

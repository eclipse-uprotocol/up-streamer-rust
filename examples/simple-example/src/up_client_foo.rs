use crate::UtransportListener;
use async_std::channel::{Receiver, Sender};
use async_std::sync::Mutex;
use async_std::task;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use up_rust::{UCode, UMessage, UMessageType, UStatus, UTransport, UUIDBuilder, UUri};
use up_streamer::UTransportBuilder;

pub struct UPClientFoo {
    protocol_receiver: Receiver<Result<UMessage, UStatus>>,
    protocol_sender: Sender<Result<UMessage, UStatus>>,
    uuid_builder: UUIDBuilder,
    listeners: Arc<Mutex<HashMap<UUri, HashMap<String, UtransportListener>>>>,
}

impl UPClientFoo {
    pub async fn new(
        protocol_receiver: Receiver<Result<UMessage, UStatus>>,
        protocol_sender: Sender<Result<UMessage, UStatus>>,
    ) -> Self {
        let listeners: Arc<Mutex<HashMap<UUri, HashMap<String, UtransportListener>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let listeners_clone = listeners.clone();
        let protocol_receiver_clone = protocol_receiver.clone();
        task::spawn(async move {
            let protocol_receiver_clone = protocol_receiver_clone.clone();
            let listeners_clone = listeners_clone.clone();

            while let Ok(received) = protocol_receiver_clone.recv().await {
                match &received {
                    Ok(msg) => {
                        let UMessage { attributes, .. } = &msg;
                        let Some(attr) = attributes.as_ref() else {
                            println!("No UAttributes!");
                            continue;
                        };

                        match attr.type_.enum_value() {
                            Ok(enum_value) => match enum_value {
                                UMessageType::UMESSAGE_TYPE_UNSPECIFIED => {
                                    println!("Type unspecified! Fail!");
                                }
                                UMessageType::UMESSAGE_TYPE_PUBLISH => {
                                    let src_uuri = attr.source.as_ref();
                                    match src_uuri {
                                        None => {
                                            println!("No source uuri!");
                                        }
                                        Some(topic) => {
                                            let listeners = listeners_clone.lock().await;
                                            let topic_listeners = listeners.get(topic);
                                            match topic_listeners {
                                                None => {
                                                    println!("No listeners for topic!");
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
                                    match sink_uuri {
                                        None => {
                                            println!("No source uuri!");
                                        }
                                        Some(topic) => {
                                            let listeners = listeners_clone.lock().await;
                                            let topic_listeners = listeners.get(topic);
                                            match topic_listeners {
                                                None => {
                                                    println!("No listeners for topic!");
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
                                    match sink_uuri {
                                        None => {
                                            println!("No source uuri!");
                                        }
                                        Some(topic) => {
                                            let listeners = listeners_clone.lock().await;
                                            let topic_listeners = listeners.get(topic);
                                            match topic_listeners {
                                                None => {
                                                    println!("No listeners for topic!");
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
            protocol_sender,
            protocol_receiver,
            uuid_builder: UUIDBuilder::new(),
            listeners,
        }
    }
}

#[async_trait]
impl UTransport for UPClientFoo {
    async fn send(&self, message: UMessage) -> Result<(), UStatus> {
        match self.protocol_sender.send(Ok(message)).await {
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
        let mut listeners = self.listeners.lock().await;
        let topic_listeners = listeners.entry(topic).or_default();
        let registration_string = self.uuid_builder.build().to_string();
        let inserted = topic_listeners.insert(registration_string.clone(), listener);

        return match inserted {
            None => Ok(registration_string),
            Some(_) => Err(UStatus::fail_with_code(
                UCode::ALREADY_EXISTS,
                "UUri and listener already registered!",
            )),
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
    protocol_receiver: Receiver<Result<UMessage, UStatus>>,
    protocol_sender: Sender<Result<UMessage, UStatus>>,
}

impl UTransportBuilderFoo {
    pub fn new(
        protocol_receiver: Receiver<Result<UMessage, UStatus>>,
        protocol_sender: Sender<Result<UMessage, UStatus>>,
    ) -> Self {
        Self {
            protocol_receiver,
            protocol_sender,
        }
    }
}

impl UTransportBuilder for UTransportBuilderFoo {
    fn build(&self) -> Result<Box<dyn UTransport>, UStatus> {
        let utransport: Box<dyn UTransport> = Box::new(task::block_on(UPClientFoo::new(
            self.protocol_receiver.clone(),
            self.protocol_sender.clone(),
        )));
        Ok(utransport)
    }
}

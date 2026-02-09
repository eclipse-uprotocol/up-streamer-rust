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

use crate::UPClientFoo;
use async_broadcast::{Receiver, Sender};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tokio::runtime::Builder;
use tokio::sync::Mutex;
use tokio_condvar::Condvar;
use tracing::{debug, error};
use up_rust::{UListener, UMessage, UStatus, UTransport, UUri, UUID};

pub type Signal = Arc<(Mutex<bool>, Condvar)>;

pub type ActiveConnections = Vec<bool>;
#[derive(PartialEq)]
pub enum ClientCommand {
    NoOp,
    Stop,
    DisconnectedFromStreamer(ActiveConnections),
    ConnectedToStreamer(ActiveConnections),
}

pub async fn check_send_receive_message_discrepancy(
    number_of_sent_messages: u64,
    number_of_messages_received: u64,
    percentage_slack: f64,
) {
    assert!(number_of_sent_messages > 0);
    assert!(number_of_messages_received > 0);

    let slack_in_message_count = number_of_sent_messages as f64 * percentage_slack;
    println!("slack_in_message_count: {slack_in_message_count}");
    println!("number_of_sent_messages: {number_of_sent_messages} number_of_messages_received: {number_of_messages_received}");
    if f64::abs(number_of_sent_messages as f64 - number_of_messages_received as f64)
        > slack_in_message_count
    {
        panic!("The discrepancy between number_of_sent_messages and number_of_message_received \
                is higher than allowable slack: number_of_sent_messages: {number_of_sent_messages}, \
                number_of_messages_received: {number_of_messages_received}, slack_in_message_count: \
                {slack_in_message_count}");
    }
}

// TODO: Fix this function to work with UUIDv7
pub async fn check_messages_in_order(messages: Arc<Mutex<Vec<UMessage>>>) {
    let messages = messages.lock().await;
    if messages.is_empty() {
        return;
    }

    // Step 1: Group messages by source UUri
    #[allow(clippy::mutable_key_type)]
    let mut grouped_messages: HashMap<UUri, Vec<&UMessage>> = HashMap::new();
    for msg in messages.iter() {
        let source_uuri = msg
            .attributes
            .as_ref()
            .unwrap()
            .source
            .as_ref()
            .unwrap()
            .clone();
        grouped_messages.entry(source_uuri).or_default().push(msg);
    }

    // Step 2: Check each group for strict increasing order of id.msb first 48 bytes
    for (source_uuri, group) in grouped_messages {
        debug!("source_uuri: {source_uuri}");
        if let Some(first_msg) = group.first() {
            let mut prev_timestamp = first_msg.attributes.as_ref().unwrap().id.msb >> 16;
            for msg in group.iter().skip(1) {
                let curr_timestamp = msg.attributes.as_ref().unwrap().id.msb >> 16;
                debug!("prev_timestamp: {prev_timestamp}, curr_timestamp: {curr_timestamp}");
                // relaxing to < instead of <= since we now do not have the counter for tie breaker
                if curr_timestamp < prev_timestamp {
                    panic!("!! -- Message ordering issue for source_uuri: {} with prev_timestamp: {} and curr_timestamp: {}-- !!", source_uuri, prev_timestamp, curr_timestamp);
                }
                prev_timestamp = curr_timestamp;
            }
        }
    }
}

pub async fn wait_for_pause(signal: Signal) {
    let (lock, cvar) = &*signal;
    let mut has_paused = lock.lock().await;
    while !*has_paused {
        debug!("inside wait_for_pause entered while loop");
        // Wait until the client signals it has paused
        has_paused = cvar.wait(has_paused).await;
        debug!("received has_paused notification");
    }
    debug!("exiting wait_for_pause");
}

pub async fn signal_to_pause(signal: Signal) {
    let (lock, cvar) = &*signal;
    let mut should_pause = lock.lock().await;
    *should_pause = true; // Indicate the client should pause
    cvar.notify_all(); // Wake up the client so it can check the condition and continue
}

pub async fn signal_to_resume(signal: Signal) {
    let (lock, cvar) = &*signal;
    let mut should_pause = lock.lock().await;
    *should_pause = false; // Indicate the client should no longer pause
    cvar.notify_all(); // Wake up the client so it can check the condition and continue
}

pub async fn reset_pause(signal: Signal) {
    {
        let (lock, _cvar) = &*signal;
        let mut has_paused = lock.lock().await;
        *has_paused = false;
    }
}

pub struct ClientConfiguration {
    pub name: String,
    pub my_client_uuri: UUri,
    pub listener: Arc<dyn UListener>,
    pub tx: Sender<Result<UMessage, UStatus>>,
    pub rx: Receiver<Result<UMessage, UStatus>>,
}

pub struct ClientMessages {
    pub notification_msgs: Vec<UMessage>,
    pub request_msgs: Vec<UMessage>,
    pub response_msgs: Vec<UMessage>,
}

pub struct ClientHistory {
    pub number_of_sends: Arc<AtomicU64>,
    pub sent_message_vec_capacity: usize,
}

pub struct ClientControl {
    pub pause_execution: Signal,
    pub execution_paused: Signal,
    pub client_command: Arc<Mutex<ClientCommand>>,
}

fn any_uuri() -> UUri {
    UUri {
        authority_name: "*".to_string(),
        ue_id: 0x0000_FFFF,     // any instance, any service
        ue_version_major: 0xFF, // any
        resource_id: 0xFFFF,    // any
        ..Default::default()
    }
}

async fn configure_client(client_configuration: &ClientConfiguration) -> UPClientFoo {
    let name = client_configuration.name.clone();
    let rx = client_configuration.rx.clone();
    let tx = client_configuration.tx.clone();
    let my_client_uuri = client_configuration.my_client_uuri.clone();
    let listener = client_configuration.listener.clone();

    let client = UPClientFoo::new(&name, rx, tx).await;

    let register_res = client
        .register_listener(&any_uuri(), Some(&my_client_uuri.clone()), listener)
        .await;
    let Ok(_registration_string) = register_res else {
        panic!("Unable to register!");
    };

    client
}

// allows us to pause execution upon command and then signal back when we've done so
async fn poll_for_new_command(
    client: &UPClientFoo,
    name: &str,
    client_control: &ClientControl,
    active_connection_listing: &mut ActiveConnections,
) -> bool {
    {
        let pause_execution = client_control.pause_execution.clone();
        let client_command = client_control.client_command.clone();
        let execution_paused = client_control.execution_paused.clone();
        let (lock, cvar) = &*pause_execution;
        let mut should_pause = lock.lock().await;
        while *should_pause {
            tokio::time::sleep(Duration::from_millis(100)).await;

            let command = client_command.lock().await;
            if *command == ClientCommand::Stop {
                let times: u64 = client.times_received.load(Ordering::SeqCst);
                println!("{name} had rx of: {times}");
                tokio::time::sleep(Duration::from_millis(1000)).await;
                return true;
            } else {
                match &*command {
                    ClientCommand::NoOp => {}
                    ClientCommand::ConnectedToStreamer(active_connections) => {
                        debug!("{} commmand: ConnectedToStreamer", &name);
                        active_connection_listing.clone_from(active_connections);
                        debug!(
                            "{} set connected_to_streamer to: {:?}",
                            &name, active_connection_listing
                        );
                    }
                    ClientCommand::DisconnectedFromStreamer(active_connections) => {
                        debug!("{} commmand: DisconnectedFromStreamer", &name);
                        active_connection_listing.clone_from(active_connections);
                        debug!(
                            "{} set connected_to_streamer to: {:?}",
                            &name, active_connection_listing
                        );
                    }
                    _ => {
                        error!(
                            "{} ClientCommand::Stop should have been handled earlier",
                            &name
                        )
                    }
                }
                {
                    let (lock_exec_pause, cvar_exec_pause) = &*execution_paused;
                    let mut has_paused = lock_exec_pause.lock().await;
                    *has_paused = true;
                    debug!("{} has_paused set to true", &name);
                    cvar_exec_pause.notify_one();
                    debug!("{} cvar_exec_pause.notify_one()", &name);
                }
                debug!("{} Loop paused. Waiting...", &name);
                should_pause = cvar.wait(should_pause).await;
                debug!("{} Got signal to pause", &name);
            }
        }

        false
    }
}

struct SendMessageContext<'a> {
    active_connection_listing: &'a ActiveConnections,
    sent_messages: &'a mut Vec<UMessage>,
    number_of_sends: &'a Arc<AtomicU64>,
}

async fn send_message_set(
    client: &UPClientFoo,
    name: &str,
    msg_type: &str,
    msg_set: &mut [UMessage],
    context: &mut SendMessageContext<'_>,
) {
    for (index, msg) in &mut msg_set.iter_mut().enumerate() {
        if let Some(attributes) = msg.attributes.as_mut() {
            let new_id = UUID::build();
            attributes.id.0 = Some(Box::new(new_id));
        }

        debug!(
            "prior to sending from client {}, the {msg_type} message: {:?}",
            &name, &msg
        );

        let send_res = client.send(msg.clone()).await;
        if send_res.is_err() {
            error!("Unable to send from client: {}", &name);
        } else if !context.active_connection_listing.is_empty()
            && context.active_connection_listing[index]
        {
            context.sent_messages.push(msg.clone());
            context.number_of_sends.fetch_add(1, Ordering::SeqCst);
            debug!(
                "{} after {msg_type} send, we have sent: {}",
                &name,
                context.number_of_sends.load(Ordering::SeqCst)
            );
        }
    }
}

pub async fn run_client(
    client_configuration: ClientConfiguration,
    mut client_messages: ClientMessages,
    client_control: ClientControl,
    client_history: ClientHistory,
) -> JoinHandle<Vec<UMessage>> {
    std::thread::spawn(move || {
        // Create a new single-threaded runtime
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime");

        runtime.block_on(async move {
            let client = configure_client(&client_configuration).await;

            let mut active_connection_listing = Vec::new();

            let start = Instant::now();

            let mut sent_messages = Vec::with_capacity(client_history.sent_message_vec_capacity);

            loop {
                debug!("top of loop");

                if poll_for_new_command(
                    &client,
                    &client_configuration.name,
                    &client_control,
                    &mut active_connection_listing,
                )
                .await
                {
                    return sent_messages;
                }

                let current = Instant::now();
                let ellapsed = current - start;

                debug!("ellapsed: {}", ellapsed.as_millis());

                debug!("-----------------------------------------------------------------------");

                let mut send_context = SendMessageContext {
                    active_connection_listing: &active_connection_listing,
                    sent_messages: &mut sent_messages,
                    number_of_sends: &client_history.number_of_sends,
                };

                send_message_set(
                    &client,
                    &client_configuration.name,
                    "Notification",
                    &mut client_messages.notification_msgs,
                    &mut send_context,
                )
                .await;
                send_message_set(
                    &client,
                    &client_configuration.name,
                    "Request",
                    &mut client_messages.request_msgs,
                    &mut send_context,
                )
                .await;
                send_message_set(
                    &client,
                    &client_configuration.name,
                    "Response",
                    &mut client_messages.response_msgs,
                    &mut send_context,
                )
                .await;

                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
    })
}

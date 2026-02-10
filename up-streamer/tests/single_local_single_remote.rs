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

mod support;

use async_broadcast::broadcast;
use futures::future::join;
use integration_test_utils::{
    check_messages_in_order, check_send_receive_message_discrepancy, local_authority,
    local_client_uuri, notification_from_local_client_for_remote_client,
    notification_from_remote_client_for_local_client, remote_authority_a, remote_client_uuri,
    request_from_local_client_for_remote_client, request_from_remote_client_for_local_client,
    reset_pause, response_from_local_client_for_remote_client,
    response_from_remote_client_for_local_client, run_client, signal_to_pause, signal_to_resume,
    wait_for_pause, wait_for_send_count, ClientCommand, ClientConfiguration, ClientControl,
    ClientHistory, ClientMessages, LocalClientListener, RemoteClientListener, UPClientFoo,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_condvar::Condvar;
use tracing::debug;
use up_rust::{UListener, UTransport};
use up_streamer::Endpoint;

const DURATION_TO_RUN_CLIENTS: u128 = 10;
const SENT_MESSAGE_VEC_CAPACITY: usize = 10_000;

async fn run_single_local_single_remote() {
    integration_test_utils::init_logging();

    // using async_broadcast to simulate communication protocol
    let (tx_1, rx_1) = broadcast(10000);
    let (tx_2, rx_2) = broadcast(10000);

    let utransport_foo: Arc<dyn UTransport> =
        Arc::new(UPClientFoo::new("upclient_foo", rx_1.clone(), tx_1.clone()).await);
    let utransport_bar: Arc<dyn UTransport> =
        Arc::new(UPClientFoo::new("upclient_bar", rx_2.clone(), tx_2.clone()).await);

    let mut ustreamer = support::make_streamer("foo_bar_streamer", 3000).await;

    // setting up endpoints between authorities and protocols
    let local_endpoint = Endpoint::new("local_endpoint", &local_authority(), utransport_foo);
    let remote_endpoint = Endpoint::new("remote_endpoint", &remote_authority_a(), utransport_bar);

    support::assert_add_rule_ok(&mut ustreamer, &local_endpoint, &remote_endpoint).await;
    support::assert_add_rule_ok(&mut ustreamer, &remote_endpoint, &local_endpoint).await;

    let local_client_listener = Arc::new(LocalClientListener::new());
    let remote_client_listener = Arc::new(RemoteClientListener::new());

    let local_client_listener_trait_obj: Arc<dyn UListener> = local_client_listener.clone();
    let remote_client_listener_trait_obj: Arc<dyn UListener> = remote_client_listener.clone();

    let all_signal_should_pause = Arc::new((Mutex::new(true), Condvar::new()));
    let local_signal_has_paused = Arc::new((Mutex::new(false), Condvar::new()));
    let remote_signal_has_paused = Arc::new((Mutex::new(false), Condvar::new()));
    let local_command = Arc::new(Mutex::new(ClientCommand::ConnectedToStreamer(vec![true])));
    let remote_command = Arc::new(Mutex::new(ClientCommand::ConnectedToStreamer(vec![true])));

    let local_sends = Arc::new(AtomicU64::new(0));
    let remote_sends = Arc::new(AtomicU64::new(0));

    // kicking off a "local_foo_client" and "remote_bar_client" in order to keep exercising
    // the streamer periodically
    let local_handle = run_client(
        ClientConfiguration {
            name: "local_foo_client".to_string(),
            my_client_uuri: local_client_uuri(10),
            listener: local_client_listener_trait_obj,
            tx: tx_1.clone(),
            rx: rx_1.clone(),
        },
        ClientMessages {
            notification_msgs: vec![notification_from_local_client_for_remote_client(
                10,
                remote_client_uuri(remote_authority_a(), 200),
            )],
            request_msgs: vec![request_from_local_client_for_remote_client(
                10,
                remote_client_uuri(remote_authority_a(), 200),
            )],
            response_msgs: vec![response_from_local_client_for_remote_client(
                10,
                remote_client_uuri(remote_authority_a(), 200),
            )],
        },
        ClientControl {
            pause_execution: all_signal_should_pause.clone(),
            execution_paused: local_signal_has_paused.clone(),
            client_command: local_command.clone(),
        },
        ClientHistory {
            number_of_sends: local_sends.clone(),
            sent_message_vec_capacity: SENT_MESSAGE_VEC_CAPACITY,
        },
    )
    .await;
    let remote_handle = run_client(
        ClientConfiguration {
            name: "remote_bar_client".to_string(),
            my_client_uuri: remote_client_uuri(remote_authority_a(), 200),
            listener: remote_client_listener_trait_obj,
            tx: tx_2.clone(),
            rx: rx_2.clone(),
        },
        ClientMessages {
            notification_msgs: vec![notification_from_remote_client_for_local_client(
                remote_client_uuri(remote_authority_a(), 200),
                10,
            )],
            request_msgs: vec![request_from_remote_client_for_local_client(
                remote_client_uuri(remote_authority_a(), 200),
                10,
            )],
            response_msgs: vec![response_from_remote_client_for_local_client(
                remote_client_uuri(remote_authority_a(), 200),
                10,
            )],
        },
        ClientControl {
            pause_execution: all_signal_should_pause.clone(),
            execution_paused: remote_signal_has_paused.clone(),
            client_command: remote_command.clone(),
        },
        ClientHistory {
            number_of_sends: remote_sends.clone(),
            sent_message_vec_capacity: SENT_MESSAGE_VEC_CAPACITY,
        },
    )
    .await;

    debug!("waiting for clients start");

    let local_paused = wait_for_pause(local_signal_has_paused.clone());
    let remote_paused = wait_for_pause(remote_signal_has_paused.clone());
    debug!("called wait_for_pause");

    join(local_paused, remote_paused).await;
    debug!("passed join on local_paused and remote_paused");

    reset_pause(local_signal_has_paused.clone()).await;
    debug!("after local has paused set to false");
    reset_pause(remote_signal_has_paused.clone()).await;
    debug!("after remote has paused set to false");

    // Now signal both clients to resume
    signal_to_resume(all_signal_should_pause.clone()).await;

    debug!("after signal_to_resume");

    let send_wait_timeout = Duration::from_millis((DURATION_TO_RUN_CLIENTS as u64).max(1_000));
    wait_for_send_count(&local_sends, 3, send_wait_timeout, "local_sends").await;
    wait_for_send_count(&remote_sends, 3, send_wait_timeout, "remote_sends").await;

    debug!("past wait on clients to run, now tell them to stop");
    {
        let mut local_command = local_command.lock().await;
        *local_command = ClientCommand::Stop;
    }
    {
        let mut remote_command = remote_command.lock().await;
        *remote_command = ClientCommand::Stop;
    }
    debug!("setting up commands for Stop");
    signal_to_pause(all_signal_should_pause).await;
    debug!("signaled for clients to pause and read command");

    let local_client_sent_messages = local_handle.join().expect("Unable to join on handle_1");
    let remote_client_sent_messages = remote_handle.join().expect("Unable to join on handle_2");

    let local_client_sent_messages_num = local_client_sent_messages.len();
    let remote_client_sent_messages_num = remote_client_sent_messages.len();

    println!("local_client_sent_messages_num: {local_client_sent_messages_num}");
    println!("remote_client_sent_messages_num: {remote_client_sent_messages_num}");

    let number_of_messages_sent = local_client_sent_messages_num + remote_client_sent_messages_num;

    println!(
        "total messages sent via reviewing messages: {}",
        number_of_messages_sent
    );

    let number_of_sent_messages =
        local_sends.load(Ordering::SeqCst) + remote_sends.load(Ordering::SeqCst);
    println!(
        "total messages sent via reviewing atomic counts: {}",
        number_of_sent_messages
    );

    let local_client_listener_msg_rx_num = local_client_listener
        .retrieve_message_store()
        .lock()
        .await
        .len();
    let remote_client_listener_msg_rx_num = remote_client_listener
        .retrieve_message_store()
        .lock()
        .await
        .len();

    println!("local_client_listener_msg_rx_num: {local_client_listener_msg_rx_num}");
    println!("remote_client_listener_msg_rx_num: {remote_client_listener_msg_rx_num}");

    let number_of_received_messages =
        local_client_listener_msg_rx_num + remote_client_listener_msg_rx_num;

    println!("total messages received by clients: {number_of_received_messages}");

    let percentage_slack = 0.000;
    check_send_receive_message_discrepancy(
        number_of_sent_messages,
        number_of_received_messages as u64,
        percentage_slack,
    )
    .await;

    check_messages_in_order(local_client_listener.retrieve_message_store()).await;
    check_messages_in_order(remote_client_listener.retrieve_message_store()).await;

    debug!("All clients finished.");
}

#[tokio::test(flavor = "multi_thread")]
async fn single_local_single_remote() {
    run_single_local_single_remote().await;
}

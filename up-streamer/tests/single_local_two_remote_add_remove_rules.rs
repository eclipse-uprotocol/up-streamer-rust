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
    notification_from_remote_client_for_local_client, remote_authority_a, remote_authority_b,
    remote_client_uuri, request_from_local_client_for_remote_client,
    request_from_remote_client_for_local_client, reset_pause,
    response_from_local_client_for_remote_client, response_from_remote_client_for_local_client,
    run_client, signal_to_pause, signal_to_resume, wait_for_pause, ClientCommand,
    ClientConfiguration, ClientControl, ClientHistory, ClientMessages, LocalClientListener,
    RemoteClientListener, UPClientFoo,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_condvar::Condvar;
use tracing::debug;
use up_rust::{UListener, UTransport};
use up_streamer::Endpoint;

const DURATION_TO_RUN_CLIENTS: u128 = 500;
const SENT_MESSAGE_VEC_CAPACITY: usize = 20_000;

async fn run_single_local_two_remote_add_remove_rules() {
    integration_test_utils::init_logging();

    // using async_broadcast to simulate communication protocol
    let (tx_1, rx_1) = broadcast(20000);
    let (tx_2, rx_2) = broadcast(20000);
    let (tx_3, rx_3) = broadcast(20000);

    let utransport_foo: Arc<dyn UTransport> =
        Arc::new(UPClientFoo::new("upclient_foo", rx_1.clone(), tx_1.clone()).await);
    let utransport_bar_1: Arc<dyn UTransport> =
        Arc::new(UPClientFoo::new("upclient_bar_1", rx_2.clone(), tx_2.clone()).await);
    let utransport_bar_2: Arc<dyn UTransport> =
        Arc::new(UPClientFoo::new("upclient_bar_2", rx_3.clone(), tx_3.clone()).await);

    let mut ustreamer = support::make_streamer("foo_bar_streamer", 3000).await;

    // setting up endpoints between authorities and protocols
    let local_endpoint = Endpoint::new("local_endpoint", &local_authority(), utransport_foo);
    let remote_endpoint_a =
        Endpoint::new("remote_endpoint_a", &remote_authority_a(), utransport_bar_1);
    let remote_endpoint_b =
        Endpoint::new("remote_endpoint_b", &remote_authority_b(), utransport_bar_2);

    support::assert_add_rule_ok(&mut ustreamer, &local_endpoint, &remote_endpoint_a).await;
    support::assert_add_rule_ok(&mut ustreamer, &remote_endpoint_a, &local_endpoint).await;

    let local_client_listener = Arc::new(LocalClientListener::new());
    let remote_a_client_listener = Arc::new(RemoteClientListener::new());
    let remote_b_client_listener = Arc::new(RemoteClientListener::new());

    let local_client_listener_trait_obj: Arc<dyn UListener> = local_client_listener.clone();
    let remote_a_client_listener_trait_obj: Arc<dyn UListener> = remote_a_client_listener.clone();
    let remote_b_client_listener_trait_obj: Arc<dyn UListener> = remote_b_client_listener.clone();

    let all_signal_should_pause = Arc::new((Mutex::new(true), Condvar::new()));
    let local_signal_has_paused = Arc::new((Mutex::new(false), Condvar::new()));
    let remote_a_signal_has_paused = Arc::new((Mutex::new(false), Condvar::new()));
    let remote_b_signal_has_paused = Arc::new((Mutex::new(false), Condvar::new()));
    let local_command = Arc::new(Mutex::new(ClientCommand::ConnectedToStreamer(vec![
        true, false,
    ])));
    let remote_a_command = Arc::new(Mutex::new(ClientCommand::ConnectedToStreamer(vec![true])));

    let local_sends = Arc::new(AtomicU64::new(0));
    let remote_a_sends = Arc::new(AtomicU64::new(0));
    let remote_b_sends = Arc::new(AtomicU64::new(0));

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
            notification_msgs: vec![
                notification_from_local_client_for_remote_client(
                    10,
                    remote_client_uuri(remote_authority_a(), 200),
                ),
                notification_from_local_client_for_remote_client(
                    10,
                    remote_client_uuri(remote_authority_b(), 200),
                ),
            ],
            request_msgs: vec![
                request_from_local_client_for_remote_client(
                    10,
                    remote_client_uuri(remote_authority_a(), 200),
                ),
                request_from_local_client_for_remote_client(
                    10,
                    remote_client_uuri(remote_authority_b(), 200),
                ),
            ],
            response_msgs: vec![
                response_from_local_client_for_remote_client(
                    10,
                    remote_client_uuri(remote_authority_a(), 200),
                ),
                response_from_local_client_for_remote_client(
                    10,
                    remote_client_uuri(remote_authority_b(), 200),
                ),
            ],
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
    let remote_a_handle = run_client(
        ClientConfiguration {
            name: "remote_a_bar_client".to_string(),
            my_client_uuri: remote_client_uuri(remote_authority_a(), 200),
            listener: remote_a_client_listener_trait_obj,
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
            execution_paused: remote_a_signal_has_paused.clone(),
            client_command: remote_a_command.clone(),
        },
        ClientHistory {
            number_of_sends: remote_a_sends.clone(),
            sent_message_vec_capacity: SENT_MESSAGE_VEC_CAPACITY,
        },
    )
    .await;

    debug!("waiting for clients start");

    let local_paused = wait_for_pause(local_signal_has_paused.clone());
    let remote_a_paused = wait_for_pause(remote_a_signal_has_paused.clone());
    debug!("called wait_for_pause");

    join(local_paused, remote_a_paused).await;
    debug!("passed join on local_paused and remote_paused");

    reset_pause(local_signal_has_paused.clone()).await;
    debug!("after local has paused set to false");
    reset_pause(remote_a_signal_has_paused.clone()).await;
    debug!("after remote_a has paused set to false");

    // Now signal clients to resume
    signal_to_resume(all_signal_should_pause.clone()).await;

    debug!("signalled to resume");

    tokio::time::sleep(Duration::from_millis(DURATION_TO_RUN_CLIENTS as u64)).await;

    {
        let mut local_command = local_command.lock().await;
        *local_command = ClientCommand::ConnectedToStreamer(vec![true, true]);
    }
    {
        let mut remote_a_command = remote_a_command.lock().await;
        *remote_a_command = ClientCommand::NoOp;
    }

    debug!("set NoOp command just to have local and remote_a pause");

    signal_to_pause(all_signal_should_pause.clone()).await;

    debug!("signalled to pause after NoOp");

    let local_paused = wait_for_pause(local_signal_has_paused.clone());
    let remote_a_paused = wait_for_pause(remote_a_signal_has_paused.clone());
    join(local_paused, remote_a_paused).await;

    debug!("finished waiting after signalling NoOp");

    let remote_b_command = Arc::new(Mutex::new(ClientCommand::ConnectedToStreamer(vec![true])));

    let remote_b_handle = run_client(
        ClientConfiguration {
            name: "remote_b_bar_client".to_string(),
            my_client_uuri: remote_client_uuri(remote_authority_b(), 200),
            listener: remote_b_client_listener_trait_obj,
            tx: tx_3.clone(),
            rx: rx_3.clone(),
        },
        ClientMessages {
            notification_msgs: vec![notification_from_remote_client_for_local_client(
                remote_client_uuri(remote_authority_b(), 200),
                10,
            )],
            request_msgs: vec![request_from_remote_client_for_local_client(
                remote_client_uuri(remote_authority_b(), 200),
                10,
            )],
            response_msgs: vec![response_from_remote_client_for_local_client(
                remote_client_uuri(remote_authority_b(), 200),
                10,
            )],
        },
        ClientControl {
            pause_execution: all_signal_should_pause.clone(),
            execution_paused: remote_b_signal_has_paused.clone(),
            client_command: remote_b_command.clone(),
        },
        ClientHistory {
            number_of_sends: remote_b_sends.clone(),
            sent_message_vec_capacity: SENT_MESSAGE_VEC_CAPACITY,
        },
    )
    .await;

    debug!("ran run_client for remote_b");

    support::assert_add_rule_ok(&mut ustreamer, &local_endpoint, &remote_endpoint_b).await;
    support::assert_add_rule_ok(&mut ustreamer, &remote_endpoint_b, &local_endpoint).await;

    debug!("added forwarding rules for remote_b <-> local");

    wait_for_pause(remote_b_signal_has_paused.clone()).await;

    debug!("remote_b signalled it paused");

    signal_to_resume(all_signal_should_pause.clone()).await;

    debug!("signalled local, remote_a, remote_b to resume");

    tokio::time::sleep(Duration::from_millis(DURATION_TO_RUN_CLIENTS as u64)).await;

    debug!("after running local, remote_a, remote_b");

    {
        let mut local_command = local_command.lock().await;
        *local_command = ClientCommand::ConnectedToStreamer(vec![false, true]);
    }
    {
        let mut remote_a_command = remote_a_command.lock().await;
        *remote_a_command = ClientCommand::Stop;
    }
    {
        let mut remote_b_command = remote_b_command.lock().await;
        *remote_b_command = ClientCommand::NoOp;
    }

    debug!(
        "set remote_a to Stop, local to disconnect remote_a for counting sends, remote_b is NoOp"
    );

    signal_to_pause(all_signal_should_pause.clone()).await;

    debug!("signalled all to pause to recognize commands");

    let remote_a_client_sent_messages = remote_a_handle
        .join()
        .expect("Unable to join on remote_a_handle");

    debug!("joined on closing of remote_a");

    let local_paused = wait_for_pause(local_signal_has_paused.clone());
    let remote_b_paused = wait_for_pause(remote_b_signal_has_paused.clone());
    debug!("called wait_for_pause for local and remote_b");

    join(local_paused, remote_b_paused).await;
    debug!("joined on local_paused and remote_b_paused");

    support::assert_delete_rule_ok(&mut ustreamer, &local_endpoint, &remote_endpoint_a).await;
    support::assert_delete_rule_ok(&mut ustreamer, &remote_endpoint_a, &local_endpoint).await;

    debug!("deleting forwarding rules local <-> remote_a");

    signal_to_resume(all_signal_should_pause.clone()).await;

    debug!("signalled all to resume: local & remote_b");

    tokio::time::sleep(Duration::from_millis(DURATION_TO_RUN_CLIENTS as u64)).await;

    {
        let mut local_command = local_command.lock().await;
        *local_command = ClientCommand::Stop;
    }
    {
        let mut remote_b_command = remote_b_command.lock().await;
        *remote_b_command = ClientCommand::Stop;
    }

    debug!("after loading Stop command for local, remote_a, remote_b");

    signal_to_pause(all_signal_should_pause.clone()).await;

    debug!("signalled local, remote_b to pause and gave them Stop command");

    let local_client_sent_messages = local_handle.join().expect("Unable to join on local_handle");
    let remote_b_client_sent_messages = remote_b_handle
        .join()
        .expect("Unable to join on remote_b_handle");

    let local_client_sent_messages_num = local_client_sent_messages.len();
    let remote_a_client_sent_messages_num = remote_a_client_sent_messages.len();
    let remote_b_client_sent_messages_num = remote_b_client_sent_messages.len();

    println!("local_client_sent_messages_num: {local_client_sent_messages_num}");
    println!("remote_a_client_sent_messages_num: {remote_a_client_sent_messages_num}");
    println!("remote_b_client_sent_messages_num: {remote_b_client_sent_messages_num}");

    let number_of_messages_received_client = local_client_sent_messages_num
        + remote_a_client_sent_messages_num
        + remote_b_client_sent_messages_num;

    println!(
        "total messages received at client: {}",
        number_of_messages_received_client
    );

    let local_client_listener_message_count = local_client_listener
        .retrieve_message_store()
        .lock()
        .await
        .len();
    let remote_a_client_listener_message_count = remote_a_client_listener
        .retrieve_message_store()
        .lock()
        .await
        .len();
    let remote_b_client_listener_message_count = remote_b_client_listener
        .retrieve_message_store()
        .lock()
        .await
        .len();

    println!(
        "local_client_listener_message_count: {}",
        local_client_listener_message_count
    );
    println!(
        "remote_a_client_listener_message_count: {}",
        remote_a_client_listener_message_count
    );
    println!(
        "remote_b_client_listener_message_count: {}",
        remote_b_client_listener_message_count
    );

    let number_of_messages_received_listeners = local_client_listener_message_count
        + remote_a_client_listener_message_count
        + remote_b_client_listener_message_count;

    println!(
        "number_of_messages_received_listeners: {}",
        number_of_messages_received_listeners
    );

    let local_send_num = local_sends.load(Ordering::SeqCst);
    let remote_a_send_num = remote_a_sends.load(Ordering::SeqCst);
    let remote_b_send_num = remote_b_sends.load(Ordering::SeqCst);

    println!("local_send_num: {}", local_send_num);
    println!("remote_a_send_num: {}", remote_a_send_num);
    println!("remote_b_send_num: {}", remote_b_send_num);

    let number_of_sent_messages = local_sends.load(Ordering::SeqCst)
        + remote_a_sends.load(Ordering::SeqCst)
        + remote_b_sends.load(Ordering::SeqCst);
    println!("total messages sent: {}", number_of_sent_messages);

    let percentage_slack = 0.000;
    check_send_receive_message_discrepancy(
        number_of_sent_messages,
        number_of_messages_received_listeners as u64,
        percentage_slack,
    )
    .await;

    println!("check local message ordering:");
    check_messages_in_order(local_client_listener.retrieve_message_store()).await;
    println!("check remote_a message ordering:");
    check_messages_in_order(remote_a_client_listener.retrieve_message_store()).await;
    println!("check remote_b message ordering:");
    check_messages_in_order(remote_b_client_listener.retrieve_message_store()).await;

    debug!("All clients finished.");
}

#[tokio::test(flavor = "multi_thread")]
async fn single_local_two_remote_add_remove_rules() {
    run_single_local_two_remote_add_remove_rules().await;
}

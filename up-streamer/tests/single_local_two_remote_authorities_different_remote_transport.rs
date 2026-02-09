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
use up_streamer::{Endpoint, UStreamer};
use usubscription_static_file::USubscriptionStaticFile;

const DURATION_TO_RUN_CLIENTS: u128 = 1_000;
const SENT_MESSAGE_VEC_CAPACITY: usize = 10_000;

async fn run_single_local_two_remote_authorities_different_remote_transport() {
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

    // setting up streamer to bridge between "foo" and "bar"
    let subscription_path =
        "../utils/usubscription-static-file/static-configs/testdata.json".to_string();
    let usubscription = Arc::new(USubscriptionStaticFile::new(subscription_path));
    let mut ustreamer = match UStreamer::new("foo_bar_streamer", 3000, usubscription) {
        Ok(streamer) => streamer,
        Err(error) => panic!("Failed to create uStreamer: {}", error),
    };

    // setting up endpoints between authorities and protocols
    let local_endpoint = Endpoint::new("local_endpoint", &local_authority(), utransport_foo);
    let remote_endpoint_a =
        Endpoint::new("remote_endpoint_a", &remote_authority_a(), utransport_bar_1);
    let remote_endpoint_b =
        Endpoint::new("remote_endpoint_b", &remote_authority_b(), utransport_bar_2);

    // adding local to remote_a routing
    let add_forwarding_rule_res = ustreamer
        .add_forwarding_rule(local_endpoint.clone(), remote_endpoint_a.clone())
        .await;
    assert!(add_forwarding_rule_res.is_ok());

    // adding remote_a to local routing
    let add_forwarding_rule_res = ustreamer
        .add_forwarding_rule(remote_endpoint_a.clone(), local_endpoint.clone())
        .await;
    assert!(add_forwarding_rule_res.is_ok());

    // adding local to remote_b routing
    let add_forwarding_rule_res = ustreamer
        .add_forwarding_rule(local_endpoint.clone(), remote_endpoint_b.clone())
        .await;
    assert!(add_forwarding_rule_res.is_ok());

    // adding remote_b to local routing
    let add_forwarding_rule_res = ustreamer
        .add_forwarding_rule(remote_endpoint_b.clone(), local_endpoint.clone())
        .await;
    assert!(add_forwarding_rule_res.is_ok());

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
        true, true,
    ])));
    let remote_a_command = Arc::new(Mutex::new(ClientCommand::ConnectedToStreamer(vec![true])));
    let remote_b_command = Arc::new(Mutex::new(ClientCommand::ConnectedToStreamer(vec![true])));

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

    debug!("waiting for clients start");

    let local_paused = wait_for_pause(local_signal_has_paused.clone());
    let remote_a_paused = wait_for_pause(remote_a_signal_has_paused.clone());
    let remote_b_paused = wait_for_pause(remote_b_signal_has_paused.clone());
    debug!("called wait_for_pause");

    join(join(local_paused, remote_a_paused), remote_b_paused).await;
    debug!("passed join on local_paused and remote_paused");

    reset_pause(local_signal_has_paused.clone()).await;
    debug!("after local has paused set to false");
    reset_pause(remote_a_signal_has_paused.clone()).await;
    debug!("after remote_a has paused set to false");
    reset_pause(remote_b_signal_has_paused.clone()).await;
    debug!("after remote_b has paused set to false");

    // Now signal both clients to resume
    signal_to_resume(all_signal_should_pause.clone()).await;

    tokio::time::sleep(Duration::from_millis(DURATION_TO_RUN_CLIENTS as u64)).await;

    {
        let mut local_command = local_command.lock().await;
        *local_command = ClientCommand::Stop;
    }
    {
        let mut remote_a_command = remote_a_command.lock().await;
        *remote_a_command = ClientCommand::Stop;
    }
    {
        let mut remote_b_command = remote_b_command.lock().await;
        *remote_b_command = ClientCommand::Stop;
    }
    signal_to_pause(all_signal_should_pause).await;

    debug!("after signal_to_resume");

    let local_client_sent_messages = local_handle.join().expect("Unable to join on local_handle");
    let remote_a_client_sent_messages = remote_a_handle
        .join()
        .expect("Unable to join on remote_a_handle");
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
async fn single_local_two_remote_authorities_different_remote_transport() {
    run_single_local_two_remote_authorities_different_remote_transport().await;
}

/*
 * Copyright (c) 2023 General Motors GTO LLC
 *
 * Licensed to the Apache Software Foundation (ASF) under one
 * or more contributor license agreements.  See the NOTICE file
 * distributed with this work for additional information
 * regarding copyright ownership.  The ASF licenses this file
 * to you under the Apache License, Version 2.0 (the
 * "License"); you may not use this file except in compliance
 * with the License.  You may obtain a copy of the License at
 *
 *   http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 * SPDX-FileType: SOURCE
 * SPDX-FileCopyrightText: 2023 General Motors GTO LLC
 * SPDX-License-Identifier: Apache-2.0
 */

#ifndef _ZENOH_RPC_CLIENT_H_
#define _ZENOH_RPC_CLIENT_H_

#include <uprotocol-cpp/rpc/RpcClient.h>
#include <uprotocol-platform-linux-zenoh/utils/ThreadPool.h>
#include <zenoh.h>
#include <src/main/proto/ustatus.pb.h>

using namespace std;
using namespace uprotocol::utransport;
using namespace uprotocol::uri;
using namespace uprotocol::v1;

class ZenohRpcClient : public RpcClient {

    public:

        /**
        * The API provides an instance of the zenoh RPC client
        * @return instance of ZenohUTransport
        */
        static ZenohRpcClient& instance(void) noexcept;

        /**
        * init the zenohRpcClient 
        * @return Returns OK on SUCCESS and ERROR on failure
        */
        UCode init() noexcept;

        /**
        * Terminates the zenoh RPC client  - the API should be called by any class that called init
        * @return Returns OK on SUCCESS and ERROR on failure
        */
        UCode term() noexcept; 

        /**
        * Support for RPC method invocation.
        * @param topic topic of the method to be invoked (i.e. the name of the API we are calling).
        * @param payload The request message to be sent to the server.
        * @param attributes Metadata for the method invocation (i.e. priority, timeout, etc.)
        * @return Returns the CompletableFuture with the result or exception.
        */
        std::future<UPayload> invokeMethod(const UUri &uri, 
                                           const UPayload &payload, 
                                           const UAttributes &attributes) noexcept;
    private:

        static UPayload handleReply(z_owned_reply_channel_t *channel);
        
        /* zenoh session handle*/
        z_owned_session_t session_;
        /* how many times uTransport was initialized*/
        atomic_uint32_t refCount_ = 0;
        
        std::mutex mutex_;
        std::shared_ptr<ThreadPool> threadPool_;

        static constexpr auto requestTimeoutMs_ = 5000;
        static constexpr auto threadPoolSize_ = size_t(10);

};

#endif /*_ZENOH_RPC_CLIENT_H_*/
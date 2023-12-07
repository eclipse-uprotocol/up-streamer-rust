#include "zenohRpcClient.h"
#include <messageBuilder.h>
#include <messageParser.h>
#include <spdlog/spdlog.h>
#include <zenoh.h>
#include <uuid/uuid.h>
#include <zenohSessionManager.h>
#include <uprotocol/uuid/serializer/UuidSerializer.h>
#include <uprotocol/uri/serializer/LongUriSerializer.h>
#include <uprotocol/transport/datamodel/UPayload.h>
#include <uprotocol/transport/datamodel/UAttributes.h>
#include <ustatus.pb.h>

using namespace uprotocol::utransport;
using namespace uprotocol::uuid;
using namespace uprotocol::uri;
using namespace uprotocol::v1;


ZenohRpcClient& ZenohRpcClient::instance(void) noexcept {
	static ZenohRpcClient rpcClient;

	return rpcClient;
}

UCode ZenohRpcClient::init() noexcept {
    if (true == init_) {
        spdlog::error("ZenohRpcClient is already initialized");
        return UCode::UNAVAILABLE;
    }

    ZenohSessionManagerConfig config;

    if (UCode::OK != ZenohSessionManager::instance().init(config)) {
        spdlog::error("zenohSessionManager::instance().init() failed");
        return UCode::UNAVAILABLE;
    }

    if (ZenohSessionManager::instance().getSession().has_value()) {
        session_ = ZenohSessionManager::instance().getSession().value();
    } else {
        return UCode::UNAVAILABLE;
    }

    threadPool_ = make_shared<ThreadPool>(threadPoolSize_);
    if (nullptr == threadPool_) {
        spdlog::error("failed to create thread pool");
        return UCode::UNAVAILABLE;
    }

    threadPool_->init();

    init_ = true;

    return UCode::OK;

}

UCode ZenohRpcClient::term() noexcept {
    if (false == init_) {
        spdlog::error("ZenohRpcClient is not initialized");
        return UCode::UNAVAILABLE;
    }

    if (UCode::OK != ZenohSessionManager::instance().term()) {
        spdlog::error("zenohSessionManager::instance().term() failed");
        return UCode::UNAVAILABLE;
    }

    threadPool_->term();

    init_ = false; 

    return UCode::OK;
} 

UPayload ZenohRpcClient::handleInvokeMethod(const z_owned_session_t &session,
                                            const UUri &uri, 
                                            const UPayload &payload, 
                                            const UAttributes &attributes) {

    auto uriHash = std::hash<std::string>{}(LongUriSerializer::serialize(uri));

    if (UMessageType::REQUEST != attributes.type()) {
        spdlog::error("Wrong message type = {}",
                      UMessageTypeToString(attributes.type()).value());

         return UPayload(nullptr,
                         0, 
                         UPayloadType::VALUE);
    }
  
    auto message = MessageBuilder::instance().build(uri,
                                                    attributes,
                                                    payload);
    if (0 == message.size()) {
        spdlog::error("MessageBuilder failure");

        return UPayload(nullptr,
                        0, 
                        UPayloadType::VALUE);
    }

    z_owned_reply_channel_t channel = zc_reply_fifo_new(16);
    
    z_get_options_t opts = z_get_options_default();
    opts.timeout_ms = requestTimeoutMs_;

    opts.value.payload = (z_bytes_t){.len =  message.size(), .start = (uint8_t *)message.data()};

    auto uuidStr = UuidSerializer::serializeToString(attributes.id());

    spdlog::debug("sending query with uid = {}", 
                  uuidStr);

    if (0 != z_get(z_loan(session), 
                   z_keyexpr(std::to_string(uriHash).c_str()),
                   "",
                   z_move(channel.send), 
                   &opts)) {
        spdlog::error("z_get failure");

        return UPayload(nullptr,
                        0, 
                        UPayloadType::VALUE);
    }  
    
    z_owned_reply_t reply = z_reply_null();
        
    //TTL exipred 
    for (z_call(channel.recv, &reply); z_check(reply); z_call(channel.recv, &reply)) {

        if (z_reply_is_ok(&reply)) {
            z_sample_t sample = z_reply_ok(&reply);
            z_owned_str_t keystr = z_keyexpr_to_string(sample.keyexpr);

            z_drop(z_move(keystr));

            auto tlvVector = MessageParser::instance().getAllTlv(sample.payload.start, 
                                                                 sample.payload.len);
     
            if (false == tlvVector.has_value()) {
                spdlog::error("getAllTlv failure");

                return UPayload(nullptr,
                                0, 
                                UPayloadType::VALUE);
            }

            auto payload = MessageParser::instance().getPayload(tlvVector.value());    
            if (false == payload.has_value()) {
                spdlog::error("getPayload failure");

                return UPayload(nullptr,
                                0, 
                                UPayloadType::VALUE);
            }

            spdlog::debug("response received");
            return std::move(payload.value());
        } else {
            spdlog::error("error received");
            
            return UPayload(nullptr,
                            0, 
                            UPayloadType::VALUE);
        }
    }

    z_drop(z_move(reply));
    z_drop(z_move(channel));    

    return UPayload(nullptr,
                    0, 
                    UPayloadType::VALUE);
}

std::future<UPayload> ZenohRpcClient::invokeMethod(const UUri &uri, 
                                                   const UPayload &payload, 
                                                   const UAttributes &attributes) noexcept {
    
    if (UMessageType::REQUEST != attributes.type()) {
        spdlog::error("Wrong message type = {}",
                      UMessageTypeToString(attributes.type()).value());
    }
    
    auto future = threadPool_->submit(handleInvokeMethod, 
                                      std::ref(session_),
                                      std::ref(uri), 
                                      std::ref(payload),
                                      std::ref(attributes));
    if (false == future.valid()) {
        spdlog::error("failed to invoke method");
    }
   
    return future; 
}
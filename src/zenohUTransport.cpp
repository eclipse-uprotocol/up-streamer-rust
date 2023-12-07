#include <zenohUTransport.h>
#include <messageBuilder.h>
#include <messageParser.h>
#include <spdlog/spdlog.h>
#include <zenohSessionManager.h>
#include <shmemMemoryMapper.h>
#include <uprotocol/uuid/serializer/UuidSerializer.h>
#include <uprotocol/uri/serializer/LongUriSerializer.h>
#include <zenoh.h>

extern void getStatus(UUri &uri, bool &status);

using namespace std;
using namespace uprotocol::uri;
using namespace uprotocol::uuid;
using namespace uprotocol::v1;

ZenohUTransport& ZenohUTransport::instance(void) noexcept {

	static ZenohUTransport zenohUtransport;

	return zenohUtransport;
}

UCode ZenohUTransport::init(ZenohUTransportConfig &transportConfig) noexcept {
    
    (void)transportConfig;

    if (true == init_) {
        spdlog::error("ZenohUTransport is already initialized");
        return UCode::UNAVAILABLE;
    }

    ZenohSessionManagerConfig sessionConfig;

    if (UCode::OK != ZenohSessionManager::instance().init(sessionConfig)) {
        spdlog::error("zenohSessionManager::instance().init() failed");
        return UCode::UNAVAILABLE;
    }

    if (ZenohSessionManager::instance().getSession().has_value()) {
        session_ = ZenohSessionManager::instance().getSession().value();
        init_ = true;
    } else {
        return UCode::UNAVAILABLE;
    }
    
    spdlog::info("ZenohUTransport init done");

    return UCode::OK;
}

UCode ZenohUTransport::term() noexcept
{
    uint8_t retries = 0;

    if (false == init_) {
        spdlog::error("ZenohUTransport is not initialized");
        return UCode::UNAVAILABLE;
    }

    termPending_ = true;

    while ((0 != pendingSendRefCnt_)
       && (termMaxRetries_ > retries)) {
        std::this_thread::sleep_for(termRetryTimeout_);
        ++retries;
    }

    if (termMaxRetries_ == retries) {
        spdlog::error("timeout elapsed while trying to terminate");
        return UCode::UNAVAILABLE;
    }

    for (auto pub : pubHandleMap_) {
        if (0 != z_undeclare_publisher(z_move(pub.second))) {
            //TODO - print the URI that failed 
            spdlog::error("z_undeclare_publisher failed");
            return UCode::INVALID_ARGUMENT;
        }
    }

    pubHandleMap_.clear();

    for (auto listenerInfo : listenerMap_) {
        for (auto sub : listenerInfo.second->subVector) {
            if (0 != z_undeclare_subscriber(z_move(sub))) {
                //TODO - print the URI that failed 
                spdlog::error("z_undeclare_publisher failed");
                return UCode::INVALID_ARGUMENT;
            } else {
                spdlog::debug("z_undeclare_subscriber done");
            }
        }

        listenerInfo.second->listenerVector.clear();
        listenerInfo.second->subVector.clear();
    }

    listenerMap_.clear();

    if (UCode::OK != ZenohSessionManager::instance().term()) {
        spdlog::error("zenohSessionManager::instance().term() failed");
        return UCode::UNAVAILABLE;
    }

    init_ = false;

    spdlog::info("ZenohUTransport term done");
    
    return UCode::OK;
}

UCode ZenohUTransport::authenticate(const UEntity &uEntity)
{
    if (false == init_) {
        spdlog::error("ZenohUTransport is not initialized");
        return UCode::UNAVAILABLE;
    }

    if (true == termPending_) {
        spdlog::error("ZenohUTransport is marked for termination");
        return UCode::UNAVAILABLE;
    }

    return UCode::OK;
}

UCode ZenohUTransport::send(const UUri &uri, 
                            const UPayload &payload,
                            const UAttributes &attributes) noexcept
{
    //todo map attributes to zenoh
    //todo thread safety
    //todo parameter validation 
    //GetSubscription(uri)


    spdlog::debug("sending uid = {}",
                  UuidSerializer::serializeToString(attributes.id()));
                  
    if (false == init_) {
        spdlog::error("ZenohUTransport is not initialized");
        return UCode::UNAVAILABLE;
    }

    if (true == termPending_) {
        spdlog::error("ZenohUTransport is marked for termination");
        return UCode::UNAVAILABLE;
    }
    
    if (false == uri.getUResource().isRPCMethod()) {
        return sendPublish(uri,
                           payload,
                           attributes);
    } else {
        return sendQueryable(uri,
                             payload,
                             attributes); 
    }
}

UCode ZenohUTransport::sendPublish(const UUri &uri, 
                                     const UPayload &payload,
                                     const UAttributes &attributes) noexcept
{
    /* TODO -
     * - Thread safe 
     * - map attributes
     * - check attributes validity
     */
    UCode status = UCode::UNAVAILABLE;
   
    do {

        if (UMessageType::PUBLISH != attributes.type()) {
            
            spdlog::error("Wrong message type = {}",
                          UMessageTypeToString(attributes.type()).value());
            return UCode::UNAVAILABLE;
        }

        ++pendingSendRefCnt_;

        /* get hash and check if the publisher for the URI is already exists */
        auto uriHash = std::hash<std::string>{}(LongUriSerializer::serialize(uri)); //uri.getHash();
        auto handleInfo = pubHandleMap_.find(uriHash);

        z_owned_publisher_t pub;
        
        if (handleInfo != pubHandleMap_.end()) {
            pub = handleInfo->second;
        } else {
            
            std::lock_guard<std::mutex> lock(pubInitMutex_);

            if (handleInfo != pubHandleMap_.end()) {
                pub = handleInfo->second;
            } else {
                pub = z_declare_publisher(z_loan(session_),
                                        z_keyexpr(std::to_string(uriHash).c_str()),
                                        nullptr);
                if (false == z_check(pub)) {
                    spdlog::error("Unable to declare Publisher for key expression!");
                    break;
                }

                pubHandleMap_[uriHash] = pub;
            }
        }

        z_publisher_put_options_t options = z_publisher_put_options_default();

        if (true == attributes.serializationHint().has_value()) {
            if (UCode::OK != mapEncoding(attributes.serializationHint().value(), 
                                           options.encoding)) {
                spdlog::error("mapEncoding failure");
                break;
            }
        } else {
            options.encoding = z_encoding(Z_ENCODING_PREFIX_TEXT_PLAIN, 
                                          nullptr);
        }

        if (UPayloadType::SHARED == payload.type()) {
            
            auto shmbuf = ShmemMemoryMapper::instance().getShmemBuffer((uint8_t*)payload.data());

            zc_owned_payload_t shmemPayload = zc_shmbuf_into_payload(z_move(shmbuf));
      
            zc_publisher_put_owned(z_loan(pub), 
                                   z_move(shmemPayload),
                                   &options);
      
        } else {
            auto message = MessageBuilder::instance().build(uri,
                                                            attributes,
                                                            payload);
            if (true == message.size()) {
                spdlog::error("MessageBuilder::instance().build failed");
                break;
            }
            
            if (0 != z_publisher_put(z_loan(pub),
                                    message.data(),
                                    message.size(),
                                    &options)){
                spdlog::error("z_publisher_put failed");
                break;
            }
        }

        status = UCode::OK;

    } while(0);
        
    --pendingSendRefCnt_;

    return status;
}

UCode ZenohUTransport::sendQueryable(const UUri &uri, 
                                       const UPayload &payload,
                                       const UAttributes &attributes) noexcept
{
    /* TODO -
     * - Thread safe 
     * - map attributes
     * - check attributes validity
     */

    if (UMessageType::RESPONSE != attributes.type()) {
        spdlog::error("a Wrong message type = {}",
                      UMessageTypeToString(attributes.type()).value());
        return UCode::UNAVAILABLE;
    }

    auto uuidStr = UuidSerializer::serializeToString(attributes.id());

    if (queryMap_.find(std::string(uuidStr)) == queryMap_.end()) {
        spdlog::error("failed to find uid = {}", 
                      string(uuidStr));
        return UCode::UNAVAILABLE;
    }

    auto query = queryMap_[std::string(uuidStr)];

    z_query_reply_options_t options = z_query_reply_options_default();

    if (true == attributes.serializationHint().has_value()) {
        if (UCode::OK != mapEncoding(attributes.serializationHint().value(), 
                                       options.encoding)) {
            spdlog::error("mapEncoding failure");
            return UCode::UNAVAILABLE;
        }
    } else {
        options.encoding = z_encoding(Z_ENCODING_PREFIX_TEXT_PLAIN, 
                                      nullptr);
    }

    auto message = MessageBuilder::instance().build(uri,
                                                    attributes,
                                                    payload);
    if (true == message.size()) {
        spdlog::error("MessageBuilder::instance().build failed");
        return UCode::UNAVAILABLE;
    }

    if (0 != z_query_reply(query, 
                           z_query_keyexpr(query), 
                           message.data(), 
                           message.size(), 
                           &options)) {
        spdlog::error("z_query_reply failed");
        return UCode::UNAVAILABLE;  
    }

    spdlog::debug("replied on query with uid = {}", 
                  std::string(uuidStr));
    
    auto keyStr = z_keyexpr_to_string(z_query_keyexpr(query));

    z_drop(z_move(keyStr));
    
    queryMap_.erase(std::string(uuidStr));

    return UCode::OK;
}

UCode ZenohUTransport::registerListener(const UUri &uri,
                                        const UListener &listener) noexcept {
   
    UCode status;
    cbArgumentType* arg;
    std::shared_ptr<ListenerContainer> listenerContainer;

    if (false == init_) {
        spdlog::error("ZenohUTransport is not initialized");
        return UCode::UNAVAILABLE;
    }

    if (true == termPending_) {
        spdlog::error("ZenohUTransport is marked for termination");
        return UCode::UNAVAILABLE;
    }

    do {

        status = UCode::OK;
        arg = nullptr;
        listenerContainer = nullptr;

        std::lock_guard<std::mutex> lock(subInitMutex_);

        auto uriHash = std::hash<std::string>{}(LongUriSerializer::serialize(uri));

        // check if URI exists 
        if (listenerMap_.find(uriHash) != listenerMap_.end()) {

            listenerContainer = listenerMap_[uriHash];

            for (const UListener *existingListenerPtr : listenerContainer->listenerVector) {
                if (existingListenerPtr == &listener) {
                    spdlog::error("listener already set for URI");
                    status = UCode::INVALID_ARGUMENT;
                    break;
                }
            }
            
            if (UCode::OK != status) {
               break;
            }
        }

        if (nullptr == listenerContainer) {
            listenerContainer = make_shared<ListenerContainer>();
            if (nullptr == listenerContainer) {
                spdlog::error("listenerContainer allocation failure");
                break;
            }
        }
        
        cbArgumentType *arg = new cbArgumentType(std::move(uri), 
                                                 this,
                                                 listener);
        if (nullptr == arg) {
            spdlog::error("failed to allocate arguments for callback");
            break;
        }

        if (false == uri.getUResource().isRPCMethod()) {

            spdlog::debug("registering listener for NON rpc URI");

            z_owned_closure_sample_t callback = z_closure(SubHandler,
                                                          OnSubscriberClose, 
                                                          arg);

            auto sub = z_declare_subscriber(z_loan(session_), 
                                            z_keyexpr(std::to_string(uriHash).c_str()),
                                            z_move(callback), 
                                            nullptr);
            if (!z_check(sub)) {
                spdlog::error("z_declare_subscriber failed");
                break;
            }
            
            listenerContainer->subVector.push_back(sub);
            listenerContainer->listenerVector.push_back(&listener);
            
        } else {

            spdlog::debug("registering listener for rpc URI");

            z_owned_closure_query_t callback = z_closure(QueryHandler, 
                                                         OnQueryClose, 
                                                         arg);
        
            auto qable = z_declare_queryable(z_loan(session_),
                                              z_keyexpr(std::to_string(uriHash).c_str()), 
                                              z_move(callback), 
                                              nullptr);
            if (!z_check(qable)) {
                spdlog::error("failed to create queryable");
                break;
            }

            listenerContainer->queryVector.push_back(qable);
            listenerContainer->listenerVector.push_back(&listener);
        }

        listenerMap_[uriHash] = listenerContainer;

    } while(0);

    if (UCode::OK != status) {
        
        if (nullptr != listenerContainer) {
            listenerContainer.reset();
        }

        if (nullptr != arg) {
            delete arg;
        }
    }

    return status;
}

UCode ZenohUTransport::unregisterListener(const UUri &uri, 
                                            const UListener &listener) noexcept {

    std::shared_ptr<ListenerContainer> listenerContainer;

    if (false == init_) {
        spdlog::error("ZenohUTransport is not initialized");
        return UCode::UNAVAILABLE;
    }

    if (true == termPending_) {
        spdlog::error("ZenohUTransport is marked for termination");
        return UCode::UNAVAILABLE;
    }

    auto uriHash = std::hash<std::string>{}(LongUriSerializer::serialize(uri));

    if (listenerMap_.find(uriHash) == listenerMap_.end()) {
        spdlog::error("uri is not registered");
        return UCode::INVALID_ARGUMENT;
    }

    listenerContainer = listenerMap_[uriHash];

    int32_t index = 0;

    // need to check with who the listener is associated
    for (const UListener *existingListenerPtr : listenerContainer->listenerVector) {

        if (&listener == existingListenerPtr) {

            listenerContainer->listenerVector.erase(listenerContainer->listenerVector.begin() + index);

            if (false == listenerContainer->subVector.empty()){
                z_undeclare_subscriber(z_move(listenerContainer->subVector[index]));
                spdlog::debug("destroyed subscriber");
                listenerContainer->subVector.erase(listenerContainer->subVector.begin() + index);
            } else {
                z_undeclare_queryable(z_move(listenerContainer->queryVector[index]));
                spdlog::debug("destroyed queryable");

                listenerContainer->queryVector.erase(listenerContainer->queryVector.begin() + index);
            }
            break;
        }

        ++index;
    }

    return UCode::OK;
}

void ZenohUTransport::SubHandler(const z_sample_t* sample,
                                 void* arg) {

    cbArgumentType *tuplePtr = static_cast<cbArgumentType*>(arg);

    auto uuri = get<0>(*tuplePtr);
    auto listener = &get<2>(*tuplePtr);

    auto tlvVector = MessageParser::instance().getAllTlv(
        sample->payload.start, 
        sample->payload.len);
    
    if (true == tlvVector.has_value()) {

        auto attributes = MessageParser::instance().getAttributes(tlvVector.value());
        if (false == attributes.has_value()) {
            spdlog::error("getAttributes failure");
            return;
        }

        auto payload = MessageParser::instance().getPayload(tlvVector.value());    
        if (false == payload.has_value()) {
            spdlog::error("getPayload failure");
            return;
        }

        if (UCode::OK != listener->onReceive(uuri, 
                                               payload.value(), 
                                               attributes.value())) {
            //todo error handling
            spdlog::error("listener->onReceive failed");
            return;
        }

    } else {

        UPayload payload(sample->payload.start, 
                         sample->payload.len,
                         UPayloadType::SHARED);

        UAttributes attributes;

        if (UCode::OK != listener->onReceive(uuri, 
                                             payload, 
                                             attributes)) {
            //todo error handling
            spdlog::error("listener->onReceive failed");
            return;
        }
    }
}

void ZenohUTransport::QueryHandler(const z_query_t *query, 
                                   void *arg) 
{
    cbArgumentType *tuplePtr = static_cast<cbArgumentType*>(arg);

    auto uuri = get<0>(*tuplePtr);
    auto instance = get<1>(*tuplePtr);
    auto listener = &get<2>(*tuplePtr);

    z_value_t payload_value = z_query_value(query);

    auto tlvVector = MessageParser::instance().getAllTlv(payload_value.payload.start, 
                                                         payload_value.payload.len);
     
    if (false == tlvVector.has_value()) {
        spdlog::error("getAllTlv failure");
        return;
    }

    auto attributes = MessageParser::instance().getAttributes(tlvVector.value());
    if (false == attributes.has_value()) {
        spdlog::error("getAttributes failure");
        return;
    }

    auto payload = MessageParser::instance().getPayload(tlvVector.value());    
    if (false == payload.has_value()) {
        spdlog::error("getPayload failure");
        return;
    }

    auto uuidStr = UuidSerializer::serializeToString(attributes->id());

    instance->queryMap_[uuidStr] = query;

    spdlog::debug("got query request uid = {}", 
                  uuidStr);

    if (UMessageType::REQUEST != attributes.value().type()) {
        spdlog::error("Wrong message type = {}",
                      UMessageTypeToString(attributes.value().type()).value());
        return;
    }
  
    if (UCode::OK != listener->onReceive(uuri, 
                                           payload.value(), 
                                           attributes.value())) {
        //TODO : define the behavior 
        spdlog::error("onReceive failure");
    }                                 
}

UCode ZenohUTransport::mapEncoding(const USerializationHint &encodingIn, 
                                     z_encoding_t &encodingOut) noexcept {

    switch (encodingIn) {
        case USerializationHint::PROTOBUF: {
            encodingOut = z_encoding(Z_ENCODING_PREFIX_APP_OCTET_STREAM,
                                     nullptr);
        }
        break;
        case USerializationHint::JSON: {
            encodingOut = z_encoding(Z_ENCODING_PREFIX_APP_JSON,
                                     nullptr);
        }
        break;
        case USerializationHint::SOMEIP: {
            encodingOut = z_encoding(Z_ENCODING_PREFIX_TEXT_PLAIN,
                                     nullptr);
        }
        break;
        case USerializationHint::RAW: {
            encodingOut = z_encoding(Z_ENCODING_PREFIX_TEXT_PLAIN,
                                     nullptr);
        }
        break;
        case USerializationHint::UNKNOWN: 
        default: {
            return UCode::UNAVAILABLE; 
        }
    }

    return UCode::OK;
}

UCode ZenohUTransport::receive(
    const UUri &uri, 
    const UPayload &payload, 
    const UAttributes &attributes) noexcept {

    (void)uri;
    (void)payload;
    (void)attributes;

    spdlog::error("not implemented");

    return UCode::UNAVAILABLE;
}

void ZenohUTransport::OnSubscriberClose(void *arg) {

    if (nullptr == arg) {
        spdlog::error("arg is nullptr");
    } else {

        cbArgumentType *info = 
            reinterpret_cast<cbArgumentType*>(arg);

        delete info;
    }
}

void ZenohUTransport::OnQueryClose(void *arg) {

    if (nullptr == arg) {
        spdlog::error("arg is nullptr");
    } else {

        cbArgumentType *info = 
            reinterpret_cast<cbArgumentType*>(arg);

        delete info;
    }
}

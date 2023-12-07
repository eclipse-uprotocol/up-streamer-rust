#include "messageBuilder.h"
#include <uprotocol/tools/base64.h>
#include <uprotocol/uuid/serializer/UuidSerializer.h>
#include <spdlog/spdlog.h>

using namespace uprotocol::uri;
using namespace uprotocol::uuid;

size_t MessageBuilder::addTag(std::vector<uint8_t>& buffer, 
                              Tag tag, 
                              const uint8_t* data, 
                              size_t size, 
                              size_t pos) {
    
    spdlog::debug("addTag : tag = {} size = {} pos = {}", 
                  (uint8_t)tag, 
                  size, 
                  pos);
 
    buffer[pos] = static_cast<uint8_t>(tag);

    std::memcpy(&buffer[pos + sizeof(uint8_t)], 
                &size, 
                sizeof(size));

    std::memcpy(&buffer[pos + sizeof(size) + sizeof(uint8_t)],
                data, 
                size);

    return pos + sizeof(uint8_t) + sizeof(size) + size;
}

template <typename T>
size_t MessageBuilder::addTag(std::vector<uint8_t>& buffer, 
                              Tag tag, 
                              const T& value, 
                              size_t pos) {
    
    const uint8_t* valueBytes = nullptr;
    size_t valueSize = 0;

    if constexpr (std::is_same_v<T, std::string>) {
        valueSize = value.length();
        valueBytes = reinterpret_cast<const uint8_t*>(value.c_str());
    } else if constexpr (std::is_same_v<T, std::vector<uint8_t>>){
        valueSize = value.size();
        valueBytes = reinterpret_cast<const uint8_t*>(value.data());
    } else {
        valueBytes = reinterpret_cast<const uint8_t*>(&value);
        valueSize = sizeof(T);
    }

    return addTag(buffer, tag, valueBytes, valueSize, pos);
}

MessageBuilder& MessageBuilder::instance(void) noexcept {
	static MessageBuilder messageBuilder;

	return messageBuilder;
}

std::vector<uint8_t> MessageBuilder::build(const UUri &uri,
                                           const UAttributes &attributes, 
                                           const UPayload &payload) noexcept {

    size_t messageSize = calculateSize(uri,
                                       attributes,
                                       payload);

    spdlog::debug("total message size = {}", 
                  messageSize);

    std::vector<uint8_t> message(messageSize);

    size_t pos = 0;

    auto uriBase64 = uprotocol::tools::Base64::encode(uri.toString());

    // pos = addTag(message, 
    //              Tag::UURI, 
    //              uriBase64, 
    //              pos);

    pos = addTag(message, 
                 Tag::ID, 
                 UuidSerializer::serializeToBytes(attributes.id()),
                 pos);       

    pos = addTag(message, 
                 Tag::TYPE, 
                 attributes.type(),
                 pos); 
                 
    pos = addTag(message, 
                 Tag::PRIORITY, 
                 attributes.priority(), 
                 pos);

    if (attributes.ttl().has_value()) {
        pos = addTag(message, 
                     Tag::TTL, 
                     attributes.ttl().value(), 
                     pos);
    }

    if (attributes.token().has_value()) {
        pos = addTag(message, 
                     Tag::TOKEN, 
                     attributes.token().value(), 
                     pos);
    }

    if (attributes.serializationHint().has_value()) {
        pos = addTag(message, 
                     Tag::HINT, 
                     attributes.serializationHint().value(), 
                     pos);
    }

    

    if (attributes.sink().has_value()) {

        auto sinkBase64 = uprotocol::tools::Base64::encode(attributes.sink().value().toString());

        pos = addTag(message, 
                     Tag::SINK, 
                     sinkBase64, 
                     pos);
    }

    if (attributes.plevel().has_value()) {
        pos = addTag(message, 
                     Tag::PLEVEL, 
                     attributes.plevel().value(), 
                     pos);
    }

    if (attributes.commstatus().has_value()) {
        pos = addTag(message, 
                     Tag::COMMSTATUS, 
                     attributes.commstatus().value(),
                     pos);
    }
    
    if (attributes.reqid().has_value()) {
         pos = addTag(message, 
                      Tag::REQID, 
                      UuidSerializer::serializeToBytes(attributes.reqid().value()), 
                      pos);
    }

    pos = addTag(message, 
                 Tag::PAYLOAD, 
                 payload.data(), 
                 payload.size(), 
                 pos);

    return message;
}

template <typename T>
void MessageBuilder::updateSize(const T& value, 
                                size_t &msgSize) noexcept {
    size_t valueSize;

    if constexpr (std::is_same_v<T, std::string>) {
        valueSize = value.length();
    } else if constexpr (std::is_same_v<T, std::vector<uint8_t>>){
        valueSize = value.size();
    } else {
        valueSize = sizeof(T);
    }

    updateSize(valueSize, 
               msgSize);
}

void MessageBuilder::updateSize(size_t size, 
    size_t &msgSize) noexcept {
    msgSize += 1;
    msgSize += sizeof(size_t);
    msgSize += size;

    spdlog::debug("updated message size = {}", 
                  msgSize);
}

size_t MessageBuilder::calculateSize(const UUri &uri, 
                                     const UAttributes &attributes, 
                                     const UPayload &payload) noexcept {

    size_t msgSize = 0;

    //updateSize(cloudevents::base64::encode(uri.toString()), msgSize);
    updateSize(UuidSerializer::serializeToBytes(attributes.id()), msgSize);
    updateSize(attributes.type(), msgSize);
    updateSize(attributes.priority(), msgSize);

    if (attributes.ttl().has_value()) {
        updateSize(attributes.ttl().value(), msgSize);
    }

    if (attributes.token().has_value()) {
        updateSize(attributes.token().value(), msgSize);
    }

    if (attributes.serializationHint().has_value()) {
        updateSize(attributes.serializationHint().value(), msgSize);
    }

    // if (attributes.sink().has_value()) {    
    //     updateSize(attributes.sink().value().getBase64(), msgSize);
    // }

    if (attributes.plevel().has_value()) {
        updateSize(attributes.plevel().value(), msgSize);
    }

    if (attributes.commstatus().has_value()) {
        updateSize(attributes.commstatus().value(), msgSize);
    }

    if (attributes.reqid().has_value()) {
        updateSize(UuidSerializer::serializeToBytes(attributes.reqid().value()), msgSize);
    }

    updateSize(payload.size(), msgSize);

    return msgSize;
}



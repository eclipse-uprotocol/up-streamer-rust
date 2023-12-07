#include "messageParser.h"
#include "messageBuilder.h"
#include <uprotocol/tools/base64.h>
#include <uprotocol/uri/datamodel/UUri.h>
#include <uprotocol/transport/datamodel/UPayload.h>
#include <uprotocol/transport/datamodel/UAttributes.h>
#include <ustatus.pb.h>
#include <uprotocol/uri/serializer/LongUriSerializer.h>
#include <uprotocol/uuid/serializer/UuidSerializer.h>
#include <spdlog/spdlog.h>

using namespace uprotocol::utransport;
using namespace uprotocol::uri;
using namespace uprotocol::uuid;

MessageParser& MessageParser::instance(void) noexcept {

	static MessageParser messageParser;

	return messageParser;
}

std::optional<std::unordered_map<Tag,TLV>> MessageParser::getAllTlv(const uint8_t *data, 
                                                                    size_t size) noexcept {

    if (nullptr == data) {
        return std::nullopt;
    }

    if (0 == size) {
        return std::nullopt;
    }

    size_t pos = 0;
    std::unordered_map<Tag, TLV> umap; 

    while (size > pos)
    {
        TLV tlv;   

        tlv.type = static_cast<Tag>(data[pos]);

        if (false == isValidTag(tlv.type)) {
            return std::nullopt;
        }
        
        pos += 1;

        std::memcpy(&tlv.length,
            data + pos,
            sizeof(tlv.length));

        pos += sizeof(tlv.length);
        
        tlv.value.insert(tlv.value.end(), 
            data + pos, 
            data + (pos + tlv.length));

        pos += tlv.length;

        umap[tlv.type] = tlv;
    }

    return umap;
}

std::optional<UUri> MessageParser::getUri(std::unordered_map<Tag,TLV> &map) noexcept {
    
    if (map.find(Tag::UURI) == map.end()){
        spdlog::error("UURI tag not found");
        return std::nullopt;
    }
    
    TLV tlv = map[Tag::UURI];

    std::string str((const char*)tlv.value.data(), 
                    tlv.length);
              
    spdlog::debug("UURI tag found length = {}", tlv.length);

    return LongUriSerializer::deserialize(uprotocol::tools::Base64::decode(str));
}


std::optional<UAttributes> MessageParser::getAttributes(std::unordered_map<Tag,TLV> &map) noexcept {

    UMessageType type;
    UPriority priority;

    if (map.find(Tag::ID) == map.end()) {
        spdlog::error("ID tag not found");
        return std::nullopt;
    }

    if (map.find(Tag::TYPE) == map.end()) {
        spdlog::error("TYPE tag not found");
        return std::nullopt;
    }

    if (map.find(Tag::PRIORITY) == map.end()) {
        spdlog::error("PRIORITY tag not found");
        return std::nullopt;
    }

    std::vector<unsigned char> idVector(map[Tag::ID].value.data(), 
                                        map[Tag::ID].value.data() + map[Tag::ID].value.size());

    UUID id = UuidSerializer::deserializeFromBytes(idVector);

    std::memcpy(
        &type,
        map[Tag::TYPE].value.data(),
        map[Tag::TYPE].length);

    std::memcpy(
        &priority,
        map[Tag::PRIORITY].value.data(),
        map[Tag::PRIORITY].length);

    UAttributesBuilder attributesBuilder(id,
                                         type,
                                         priority);

    if (map.find(Tag::TTL) != map.end()) {
        int value = *reinterpret_cast<int*>(map[Tag::ID].value.data());
        attributesBuilder.withTtl(value);
    }

    if (map.find(Tag::PLEVEL) != map.end()) {
        int value = *reinterpret_cast<int*>(map[Tag::PLEVEL].value.data());
        attributesBuilder.withPermissionLevel(value);
    }

    if (map.find(Tag::COMMSTATUS) != map.end()) {
        int value = *reinterpret_cast<int*>(map[Tag::COMMSTATUS].value.data());
        attributesBuilder.withCommStatus(value);
    }

    // if (map.find(Tag::TOKEN) != map.end()) {
    //     std::string str(reinterpret_cast<char*>(map[Tag::COMMSTATUS].value.data()), map[Tag::COMMSTATUS].length);
    //     attributesBuilder.withToken(value);
    // }

   // UAttributesBuilder& withHint(const USerializationHint& hint)

    //UAttributesBuilder& withSink(const UUri& sink)



   // UAttributesBuilder& withReqId(const UUID& reqid)

    return attributesBuilder.build();
}

std::optional<UPayload> MessageParser::getPayload(std::unordered_map<Tag,TLV> &map) noexcept
{
    if (map.find(Tag::PAYLOAD) == map.end()) {
        spdlog::error("PAYLOAD tag not found");
        return std::nullopt;
    }

    TLV tlvPayload = map[Tag::PAYLOAD];

    spdlog::debug("PAYLOAD tag found length = {}", tlvPayload.length);

    return UPayload(tlvPayload.value.data(), 
                    tlvPayload.length,
                    UPayloadType::VALUE);
}

bool MessageParser::isValidTag(Tag value) {
    return value >= Tag::UURI && value <= Tag::PAYLOAD;
}
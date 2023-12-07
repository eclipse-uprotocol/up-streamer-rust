#ifndef _MESSAGEPARSER_H_
#define _MESSAGEPARSER_H_

#include <unordered_map>
#include <vector>
#include "messageBuilder.h"
#include <uprotocol/tools/base64.h>
#include <uprotocol/uri/datamodel/UUri.h>
#include <uprotocol/transport/datamodel/UPayload.h>
#include <uprotocol/transport/datamodel/UAttributes.h>
#include <ustatus.pb.h>

using namespace uprotocol::utransport;
using namespace uprotocol::uri;
using namespace uprotocol::v1;

struct TLV {
    Tag type;
    size_t length;
    std::vector<uint8_t> value;
};

class MessageParser
{
    public:

        static MessageParser& instance(void) noexcept;

        std::optional<std::unordered_map<Tag,TLV>> getAllTlv(const uint8_t *data, 
                                                             size_t         size) noexcept;

        std::optional<UUri> getUri(std::unordered_map<Tag,TLV> &map) noexcept;

        std::optional<UAttributes> getAttributes(std::unordered_map<Tag,TLV> &map) noexcept;
        
        std::optional<UPayload> getPayload(std::unordered_map<Tag,TLV> &map) noexcept;       
    
    private:

        bool isValidTag(Tag value);
};

#endif /*_MESSAGEPARSER_H_ */
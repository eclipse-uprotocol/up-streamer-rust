#ifndef _MESSAGEBUILDER_H_
#define _MESSAGEBUILDER_H_

#include <cstdint>
#include <cstring>
#include <uprotocol/tools/base64.h>
#include <uprotocol/uri/datamodel/UUri.h>
#include <uprotocol/transport/datamodel/UPayload.h>
#include <uprotocol/transport/datamodel/UAttributes.h>
#include <ustatus.pb.h>

using namespace uprotocol::utransport;
using namespace uprotocol::uri;

enum class Tag
{
    UURI = 0,
    ID = 1, 
    TYPE = 2,
    PRIORITY = 3,
    TTL = 4,
    TOKEN = 5,
    HINT = 6,
    SINK = 7,
    PLEVEL = 8,
    COMMSTATUS = 9,
    REQID = 10,
    PAYLOAD = 11, 

    UNDEFINED = 12
};

class MessageBuilder 
{
    public:

        static MessageBuilder& instance(void) noexcept;

        std::vector<uint8_t> build(const UUri &uri,
                                   const UAttributes &attributes, 
                                   const UPayload &payload) noexcept;

    private:

        size_t calculateSize(const UUri &uri, 
                             const UAttributes &attributes, 
                             const UPayload &payload) noexcept;
        
        template <typename T>
        void updateSize(const T &value, 
                        size_t &msgSize) noexcept;

        void updateSize(size_t size, 
                        size_t &msgSize) noexcept;

        size_t addTag(std::vector<uint8_t>& buffer,
                      Tag tag, 
                      const uint8_t* data, 
                      size_t size, 
                      size_t pos);
        
        template <typename T>
            size_t addTag(std::vector<uint8_t>& buffer, 
                          Tag tag, 
                          const T& value, 
                          size_t pos);
};

#endif /* _MESSAGE_BUILDER_H_ */
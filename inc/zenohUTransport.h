#ifndef _ZENOH_UTRANSPORT_
#define _ZENOH_UTRANSPORT_

#include <stdint.h>
#include <cstddef>
#include <unordered_map>
#include <atomic>
#include <zenoh.h>
#include <transport/UTransport.h>

using namespace uprotocol::v1;
using namespace std;
using namespace uprotocol::utransport;

class ZenohUTransportConfig
{

};

class ListenerContainer {
    public:
        std::vector<z_owned_subscriber_t> subVector;
        std::vector<z_owned_queryable_t> queryVector;
        std::vector<const UListener*> listenerVector;
};

class ZenohUTransport : public UTransport {

    public:

		static ZenohUTransport& instance(void) noexcept;

        UCode init(ZenohUTransportConfig &transportConfig) noexcept;

        UCode term() noexcept; 

        UCode authenticate(const UEntity &uEntity);

        UCode send(const UUri &uri, 
                   const UPayload &payload,
                   const UAttributes &attributes) noexcept;

        UCode registerListener(const UUri &uri,
                               const UListener &listener) noexcept;

        UCode unregisterListener(const UUri &uri, 
                                 const UListener &listener) noexcept;

        UCode receive(const UUri &uri, 
                      const UPayload &payload, 
                      const UAttributes &attributes) noexcept;

    private:

        static void OnSubscriberClose(void *arg);

        static void OnQueryClose(void *arg);

        static void QueryHandler(const z_query_t *query, 
                                 void *context);

        static void SubHandler(const z_sample_t* sample,
                               void* arg);

        UCode sendPublish(const UUri &uri, 
                            const UPayload &payload,
                            const UAttributes &attributes) noexcept;

        UCode sendQueryable(const UUri &uri, 
                              const UPayload &payload,
                              const UAttributes &attributes) noexcept;

        UCode mapEncoding(const USerializationHint &encodingIn, 
                            z_encoding_t &encodingOut) noexcept;

        z_owned_session_t session_;
        bool init_ = false; 
        atomic_bool termPending_ = false;
        atomic_uint32_t pendingSendRefCnt_ = 0;
    
        using uuriKey = size_t;     
        using uuidStr = std::string;

        unordered_map<uuriKey, z_owned_publisher_t> pubHandleMap_;
        unordered_map<uuriKey, std::shared_ptr<ListenerContainer>> listenerMap_;  
        unordered_map<uuidStr, const z_query_t *> queryMap_;
        unordered_map<uuriKey, bool> authorized_;

        std::mutex pubInitMutex_;
        std::mutex subInitMutex_;

        static constexpr auto termMaxRetries_ = size_t(10);
        static constexpr auto termRetryTimeout_ = std::chrono::milliseconds(100);

        using cbArgumentType = std::tuple<const UUri&, ZenohUTransport*, const UListener&>;
};

#endif /*_ZENOH_UTRANSPORT_*/
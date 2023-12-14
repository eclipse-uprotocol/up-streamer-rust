#include <uprotocol-platform-linux-zenoh/transport/zenohUTransport.h>
#include <uprotocol/uuid/serializer/UuidSerializer.h>
#include <csignal>
#include <core/usubscription/v3/usubscription.pb.h>
#include <uri.pb.h>
#include <ustatus.pb.h>
#include <google/protobuf/descriptor.h> // Include the descriptor header


using namespace uprotocol::utransport;
using namespace uprotocol::uuid;
using namespace uprotocol::v1;
using namespace uprotocol::core::usubscription::v3;
//using namespace uprotocol::v1;

bool gTerminate = false; 

void signalHandler(int signal) {
    if (signal == SIGINT) {
        std::cout << "Ctrl+C received. Exiting..." << std::endl;
        gTerminate = true; 
    }
}

class TimeListener : public UListener {
    
    UCode onReceive(const uprotocol::uri::UUri &uri, 
                      const UPayload &payload, 
                      const UAttributes &attributes) const {
                        
        (void)uri;

        uint64_t timeInMilliseconds;
      
        memcpy(
            &timeInMilliseconds,
            payload.data(),
            payload.size());

        spdlog::info("time = {}",
                     timeInMilliseconds);

        return UCode::OK;
    }
};

class RandomListener : public UListener {

    UCode onReceive(
        const uprotocol::uri::UUri &uri, 
        const UPayload &payload, 
        const UAttributes &attributes) const {

        (void)uri;

        uint32_t random;

        memcpy(
            &random,
            payload.data(),
            payload.size());

        spdlog::info("type = {} priority = {} random = {}",
            UMessageTypeToString(attributes.type()).value(), 
            UPriorityToString(attributes.priority()).value(), 
            random);

        return UCode::OK;
    }
};

class CounterListener : public UListener {

    UCode onReceive(
        const uprotocol::uri::UUri &uri, 
        const UPayload &payload, 
        const UAttributes &attributes) const {

        (void)uri;

        uint8_t counter;

        memcpy(
            &counter,
            payload.data(),
            payload.size());

        spdlog::info("type = {} priority = {} counter = {}",
            UMessageTypeToString(attributes.type()).value(), 
            UPriorityToString(attributes.priority()).value(), 
            counter);

        return UCode::OK;
    }
};

int main(int argc, char **argv) {

    signal(SIGINT, 
           signalHandler);

    TimeListener timeListener;
    RandomListener randomListener;
    CounterListener counterListener;

    if (1 < argc) {
        if (0 == strcmp("-d", argv[1])) {
            spdlog::set_level(spdlog::level::debug);
        }
    }

    ZenohUTransportConfig config;

    if (UCode::OK != ZenohUTransport::instance().init(config)) {
        spdlog::error("ZenohUTransport::instance().init failed");
        return -1;
    }

    auto use = uprotocol::uri::UEntity::longFormat("test.app");

    auto timeUri = uprotocol::uri::UUri(uprotocol::uri::UAuthority::local(), 
                        use,
                        uprotocol::uri::UResource::longFormat("milliseconds"));
                                       
    auto randomUri = uprotocol::uri::UUri(uprotocol::uri::UAuthority::local(), 
                          use,
                          uprotocol::uri::UResource::longFormat("32bit"));
    
    auto counterUri = uprotocol::uri::UUri(uprotocol::uri::UAuthority::local(), 
                           use,
                           uprotocol::uri::UResource::longFormat("counter"));

    if (UCode::OK != ZenohUTransport::instance().registerListener(timeUri, 
                                                                    timeListener)) {

        spdlog::error("ZenohUTransport::instance().registerListener failed");
        return -1;
    }
       
    if (UCode::OK != ZenohUTransport::instance().registerListener(randomUri, 
                                                                    randomListener)) {

        spdlog::error("ZenohUTransport::instance().registerListener failed");
        return -1;
    }

    if (UCode::OK != ZenohUTransport::instance().registerListener(counterUri, 
                                                                    counterListener)) {

        spdlog::error("ZenohUTransport::instance().registerListener failed");
        return -1;
    }

    while(!gTerminate) {
        sleep(1);
    }
   
    if (UCode::OK != ZenohUTransport::instance().unregisterListener(timeUri, 
                                                                      timeListener)) {

        spdlog::error("ZenohUTransport::instance().unregisterListener failed");
        return -1;
    }

    if (UCode::OK != ZenohUTransport::instance().unregisterListener(randomUri, 
                                                                      randomListener)) {

        spdlog::error("ZenohUTransport::instance().unregisterListener failed");
        return -1;
    }

    if (UCode::OK != ZenohUTransport::instance().unregisterListener(counterUri, 
                                                                      counterListener)) {

        spdlog::error("ZenohUTransport::instance().unregisterListener failed");
        return -1;
    }

    if (UCode::OK != ZenohUTransport::instance().term()) {
        spdlog::error("ZenohUTransport::instance().term failed");
        return -1;
    }

    return 0;
}

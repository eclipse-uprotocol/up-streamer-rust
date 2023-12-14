#include <uprotocol-platform-linux-zenoh/transport/zenohUTransport.h>
#include <csignal>
#include <src/main/proto/ustatus.pb.h>

using namespace uprotocol::utransport;
using namespace uprotocol::uri;
using namespace uprotocol::uuid;
using namespace uprotocol::v1;

bool gTerminate = false; 

void signalHandler(int signal) {
    if (signal == SIGINT) {
        std::cout << "Ctrl+C received. Exiting..." << std::endl;
        gTerminate = true; 
    }
}


static uint8_t* getTime()
{
    auto currentTime = std::chrono::system_clock::now();
    auto duration = currentTime.time_since_epoch();
    
    auto timeMilli = std::chrono::duration_cast<std::chrono::milliseconds>(duration).count();

    static uint8_t buf[8];

    memcpy(
        buf,
        &timeMilli,
        sizeof(timeMilli));

    return buf;
}

static uint8_t* getRandom()
{
    int32_t val = rand();

    static uint8_t buf[4];

    memcpy(
        buf,
        &val,
        sizeof(val));

    return buf;
}

static uint8_t* getCounter()
{
    static uint8_t counter = 0;

    ++counter;

    return &counter;
}

UCode sendMessage(
    UUri &uri, 
    uint8_t *buffer,
    size_t size)
{
    UUID uuid;
        
    auto type = UMessageType::PUBLISH;
    auto priority = UPriority::STANDARD;

    UAttributesBuilder builder(uuid, 
                               type, 
                               priority);

    UAttributes attributes = builder.build();

    UPayload payload(buffer,
                     size,
                     UPayloadType::VALUE);
   
    if (UCode::OK != ZenohUTransport::instance().send(uri, 
                                                        payload, 
                                                        attributes)) {
        spdlog::error("ZenohUTransport::instance().send failed");
        return UCode::UNAVAILABLE;
    }

    return UCode::OK;
}

int main(int argc, char **argv) {

    signal(SIGINT, 
           signalHandler);

    if (1 < argc) {
        if (0 == strcmp("-d", argv[1])) {
            spdlog::set_level(spdlog::level::debug);
        }
    }

    size_t constexpr maxIterations = 30;

    ZenohUTransportConfig config;

    if (UCode::OK != ZenohUTransport::instance().init(config)) {
        spdlog::error("ZenohUTransport::instance().init failed");
        return -1;
    }

    auto timeUri = UUri(UAuthority::local(), 
                        UEntity::longFormat("test.app"),
                        UResource::longFormat("milliseconds"));

    auto randomUri = UUri(UAuthority::local(), 
                          UEntity::longFormat("test.app"),
                          UResource::longFormat("32bit"));
    
    auto counterUri = UUri(UAuthority::local(),
                           UEntity::longFormat("test.app"),
                           UResource::longFormat("counter"));

    while(!gTerminate) {

         if (UCode::OK != sendMessage(timeUri,
                                        getTime(),
                                        8)) {
            spdlog::error("sendMessage failed");
            break;
         }

         if (UCode::OK != sendMessage(randomUri,
                                        getRandom(),
                                        4)) {
            spdlog::error("sendMessage failed");
            break;
         }

         if (UCode::OK != sendMessage(counterUri,
                                        getCounter(),
                                        1)) {
            spdlog::error("sendMessage failed");
            break;
         }

         sleep(1);
    }

    if (UCode::OK != ZenohUTransport::instance().term()) {
        spdlog::error("ZenohUTransport::instance().term() failed");
        return -1;
    }

    return 0;
}

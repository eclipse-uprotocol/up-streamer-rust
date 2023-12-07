#include <zenohUTransport.h>
#include <shmemMemoryAllocator.h>

using namespace uprotocol::utransport;
using namespace uprotocol::uri;
using namespace uprotocol::uuid;
using namespace uprotocol::v1;

static uint8_t* getTime(ShmemMemoryAllocator &shmemAllocator)
{
    auto currentTime = std::chrono::system_clock::now();
    auto duration = currentTime.time_since_epoch();
    
    auto timeMilli = std::chrono::duration_cast<std::chrono::milliseconds>(duration).count();

    auto buf = shmemAllocator.allocate();

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
    size_t size,
    UPayloadType payloadType)
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
                     payloadType);
   
    if (UCode::OK != ZenohUTransport::instance().send(uri, 
                                                        payload, 
                                                        attributes))
    {
        spdlog::error("ZenohUTransport::instance().send failed");
        return UCode::UNAVAILABLE;
    }

    return UCode::OK;
}

int main(int argc, char **argv) {

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

    auto use = UEntity::longFormat("test.app");

    auto timeUri = UUri(UAuthority::local(), 
                        use,
                        UResource::longFormat("milliseconds"));

    ShmemMemoryAllocator timeUriAllocator(timeUri,
                                          8 /*element size*/,
                                          2 /* queue size */);

    if (UCode::OK != timeUriAllocator.initialize()) {
        spdlog::error("timeUriAllocator.initialize failed");
        return -1;
    }

    auto randomUri = UUri(UAuthority::local(), 
                          use,
                          UResource::longFormat("32bit"));
    
    auto counterUri = UUri(UAuthority::local(),
                           use,
                           UResource::longFormat("counter"));

    for (size_t i = 0 ; i < maxIterations ; ++i) {

        auto bufTime = getTime(timeUriAllocator);

        if (UCode::OK != sendMessage(timeUri,
                                       bufTime,
                                       8,
                                       UPayloadType::SHARED)) {
            spdlog::error("sendMessage failed");
            break;
        }

        timeUriAllocator.free(bufTime);

        if (UCode::OK != sendMessage(randomUri,
                                       getRandom(),
                                       4,
                                       UPayloadType::VALUE)) {
            spdlog::error("sendMessage failed");
            break;
        }

        if (UCode::OK != sendMessage(counterUri,
                                       getCounter(),
                                       1,
                                       UPayloadType::VALUE)) {
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

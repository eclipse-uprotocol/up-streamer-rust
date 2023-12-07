#include <zenohUTransport.h>
#include <zenohRpcClient.h>
#include <chrono>
#include <csignal>

using namespace uprotocol::utransport;
using namespace uprotocol::uuid;
using namespace uprotocol::v1;

bool gTerminate = false; 

void signalHandler(int signal) {
    if (signal == SIGINT) {
        std::cout << "Ctrl+C received. Exiting..." << std::endl;
        gTerminate = true; 
    }
}

UPayload sendRPC(UUri &uri)
{
    UUID uuid;
        
    auto type = UMessageType::REQUEST;
    auto priority = UPriority::STANDARD;

    UAttributesBuilder builder(uuid, 
                               type, 
                               priority);

    UAttributes attributes = builder.build();

    uint8_t buffer[1];

    UPayload payload(buffer,
                     sizeof(buffer),
                     UPayloadType::VALUE);

    std::future<UPayload> result = ZenohRpcClient::instance().invokeMethod(uri, 
                                                                           payload, 
                                                                           attributes);
    if (false == result.valid()) {
        spdlog::error("future is invalid");
        return UPayload(nullptr, 
                        0,
                        UPayloadType::UNDEFINED);   
    }

    result.wait();

    return result.get();
}

int main(int argc, char **argv)
{
    signal(SIGINT, 
           signalHandler);

    if (1 < argc) {
        if (0 == strcmp("-d", argv[1])) {
            spdlog::set_level(spdlog::level::debug);
        }
    }

    if (UCode::OK != ZenohRpcClient::instance().init())
    {
        spdlog::error("ZenohRpcClient::instance().init failed");
        return -1;
    }

    auto use = UEntity::longFormat("test_rpc.app");

    auto rpcUri = UUri(
        UAuthority::local(), use,
        UResource::forRpcRequest("getTime"));


    while (!gTerminate) {
        auto response = sendRPC(rpcUri);

        uint64_t milliseconds;

        if (nullptr != response.data()){
            memcpy(
                &milliseconds,
                response.data(),
                response.size());

            spdlog::info("received = {}", 
                        milliseconds);
        }
        sleep(1);
    }

    if (UCode::OK != ZenohRpcClient::instance().term()) {
        spdlog::error("ZenohRpcClient::instance().term() failed");
    }

    return 0;
}

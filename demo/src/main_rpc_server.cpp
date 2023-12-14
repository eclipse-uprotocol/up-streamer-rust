#include <uprotocol-platform-linux-zenoh/transport/zenohUTransport.h>
#include <csignal>
#include <utils/uprotocol-platform-linux-zenoh/utils/ConcurrentCyclicQueue.h>

using namespace uprotocol::utransport;
using namespace uprotocol::v1;

using RpcRequest = std::pair<std::unique_ptr<UUri>, std::unique_ptr<UAttributes>>;

bool gTerminate = false; 

void signalHandler(int signal) {
    if (signal == SIGINT) {
        std::cout << "Ctrl+C received. Exiting..." << std::endl;
        gTerminate = true; 
    }
}

#ifdef ASYNC_RESPONSE
class RpcListener : public UListener
{
    public:

        RpcListener() {

            handlerThread_ = std::thread(RpcHandler,
                                         std::ref(queue_),
                                         std::ref(shutdown_));
        }

        ~RpcListener() {
            shutdown_ = true;
            if(handlerThread_.joinable()) {
                handlerThread_.join();
            }
        }

        UCode onReceive(
            const UUri &uri, 
            const UPayload &payload, 
            const UAttributes &attributes) const {
    
            RpcRequest rpcInput(std::make_unique<UUri>(std::move(uri)), 
                                std::make_unique<UAttributes>(std::move(attributes)));
            
            queue_.push(rpcInput);

            return UCode::OK;
        }
    
    private:

        static void RpcHandler(ConcurrentCyclicQueue<RpcRequest> &queue,
                               bool &shutdown) {
            while (!shutdown) {

                RpcRequest req;

                if (true == queue.waitPop(req)) {

                    auto currentTime = std::chrono::system_clock::now();
                    auto duration = currentTime.time_since_epoch();
                    
                    auto timeMilli = std::chrono::duration_cast<std::chrono::milliseconds>(duration).count();

                    static uint8_t buf[8];

                    memcpy(
                        buf,
                        &timeMilli,
                        sizeof(timeMilli));

                    UPayload response(buf,
                                      sizeof(buf),
                                      UPayloadType::VALUE);

                    auto uri = req.first.get();
                    auto attributes = req.second.get();

                    auto type = UMessageType::RESPONSE;
                    auto priority = UPriority::STANDARD;

                    UAttributesBuilder builder(attributes->id(), 
                                               type, 
                                               priority);
                    ZenohUTransport::instance().send(*uri,
                                                     response,
                                                     builder.build());
                }
            }
        }

        ConcurrentCyclicQueue<RpcRequest> queue_{1, std::chrono::milliseconds(100)};
        std::thread handlerThread_;
        bool shutdown_ = false; 
};

#else
class RpcListener : public UListener
{
    UCode onReceive(
        const UUri &uri, 
        const UPayload &payload, 
        const UAttributes &attributes) const
    {
        auto currentTime = std::chrono::system_clock::now();
        auto duration = currentTime.time_since_epoch();
        
        auto timeMilli = std::chrono::duration_cast<std::chrono::milliseconds>(duration).count();

        static uint8_t buf[8];

        memcpy(
            buf,
            &timeMilli,
            sizeof(timeMilli));

        UPayload response(buf,
                          sizeof(buf),
                          UPayloadType::VALUE);

        auto type = UMessageType::RESPONSE;
        auto priority = UPriority::STANDARD;

        UAttributesBuilder builder(attributes.id(), 
                                   type, 
                                   priority);

        ZenohUTransport::instance().send(uri,
                                         response,
                                         builder.build());

        return UCode::OK;
    }
    
};
#endif 

int main(int argc, char **argv)
{
    signal(SIGINT, 
           signalHandler);

    RpcListener rpcListener;

    if (1 < argc) {
        if (0 == strcmp("-d", argv[1])) {
            spdlog::set_level(spdlog::level::debug);
        }
    }

    ZenohUTransportConfig config;

    if (UCode::OK != ZenohUTransport::instance().init(config)) {
        spdlog::error("ZenohRpcServer::instance().init failed");
        return -1;
    }

    auto rpcUri = UUri(UAuthority::local(), 
                       UEntity::longFormat("test_rpc.app"),
                       UResource::forRpcRequest("getTime"));;

    if (UCode::OK != ZenohUTransport::instance().registerListener(rpcUri, 
                                                                    rpcListener)) {
        spdlog::error("ZenohRpcServer::instance().registerListener failed");
        return -1;
    }

    while (!gTerminate) {
        sleep(1);
    }

    if (UCode::OK != ZenohUTransport::instance().unregisterListener(rpcUri, 
                                                                      rpcListener)) {
        spdlog::error("ZenohRpcServer::instance().unregisterListener failed");
        return -1;
    }

    if (UCode::OK != ZenohUTransport::instance().term()) {
        spdlog::error("ZenohUTransport::instance().term failed");
        return -1;
    }

    return 0;
}

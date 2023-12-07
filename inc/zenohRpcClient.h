#ifndef _ZENOH_RPC_CLIENT_H_
#define _ZENOH_RPC_CLIENT_H_

#include <rpc/RpcClient.h>
#include <ustatus.pb.h>
#include <ThreadPool.h>
#include <zenoh.h>

using namespace std;
using namespace uprotocol::utransport;
using namespace uprotocol::uri;
using namespace uprotocol::v1;

class ZenohRpcClient : public RpcClient {
    public:
        static ZenohRpcClient& instance(void) noexcept;

        UCode init() noexcept;

        UCode term() noexcept; 

        std::future<UPayload> invokeMethod(const UUri &uri, 
                                           const UPayload &payload, 
                                           const UAttributes &attributes) noexcept;
    private:

       static UPayload handleInvokeMethod(const z_owned_session_t &session,
                                          const UUri &uri, 
                                          const UPayload &payload, 
                                          const UAttributes &attributes);

        bool init_;
        z_owned_session_t session_;
        std::shared_ptr<ThreadPool> threadPool_;

        static constexpr auto requestTimeoutMs_ = 5000;
        static constexpr auto threadPoolSize_ = size_t(10);

};

#endif /*_ZENOH_RPC_CLIENT_H_*/
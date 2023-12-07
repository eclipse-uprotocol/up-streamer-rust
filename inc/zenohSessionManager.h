#ifndef _ZENOH_SESSION_MANAGER_H_
#define _ZENOH_SESSION_MANAGER_H_

#include <atomic>
#include <zenoh.h>
#include <transport/datamodel/UPayload.h>
#include <transport/datamodel/UAttributes.h>
#include <ustatus.pb.h>

using namespace std;
using namespace uprotocol::utransport;
using namespace uprotocol::v1;

struct ZenohSessionManagerConfig
{
    std::string connectKey;
    std::string listenKey;
};

class ZenohSessionManager
{
    public:

        static ZenohSessionManager& instance() noexcept;

        UCode init(ZenohSessionManagerConfig &sessionConfig) noexcept;

        UCode term() noexcept;

        std::optional<z_owned_session_t> getSession() noexcept;

    private:

        z_owned_session_t session_;
        atomic_uint32_t refCount_;
};

#endif /*_ZENOH_SESSION_MANAGER_H_*/
#include "zenohSessionManager.h"
#include <spdlog/spdlog.h>

ZenohSessionManager& ZenohSessionManager::instance(void) noexcept {

	static ZenohSessionManager sessionManager;

	return sessionManager;
}

UCode ZenohSessionManager::init(ZenohSessionManagerConfig &sessionConfig) noexcept {

    if (0 == refCount_) {
        ++refCount_;

        z_owned_config_t config = z_config_default();
    
        if (0 < sessionConfig.connectKey.length()) {
            if (0 > zc_config_insert_json(z_loan(config), 
                                          Z_CONFIG_CONNECT_KEY, 
                                          sessionConfig.connectKey.c_str())) {
                spdlog::error("zc_config_insert_json (Z_CONFIG_CONNECT_KEY) failed");
                return UCode::UNAVAILABLE;
            }
        }

        if (0 < sessionConfig.listenKey.length()) {
            if (0 > zc_config_insert_json(z_loan(config), 
                                        Z_CONFIG_LISTEN_KEY, 
                                        sessionConfig.listenKey.c_str())) {
                spdlog::error("zc_config_insert_json (Z_CONFIG_LISTEN_KEY) failed");
                return UCode::UNAVAILABLE;
            }
        }

        session_ = z_open(z_move(config));

        if (false == z_check(session_)) {
            spdlog::error("z_open failed");
            return UCode::INVALID_ARGUMENT;
        }
    }
    
    spdlog::info("ZenohSessionManager::init done , ref count {}", 
                 refCount_);
    
    return UCode::OK;
}

UCode ZenohSessionManager::term() noexcept {
    
    if (0 < refCount_) {

        --refCount_;

        if (0 == refCount_) {
            spdlog::info("ZenohSessionManager::term() done");
            z_close(z_move(session_));
        }
    }

    return UCode::OK;
}

std::optional<z_owned_session_t> ZenohSessionManager::getSession() noexcept {

    if (0 < refCount_) {
        return session_;
    }

    return std::nullopt;
}
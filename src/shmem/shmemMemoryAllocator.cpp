#include <cstdint>
#include <optional>
#include <shmemMemoryAllocator.h>
#include <shmemMemoryMapper.h>
#include <zenohSessionManager.h>
#include <ustatus.pb.h>
#include <uprotocol/uri/serializer/LongUriSerializer.h>
#include <spdlog/spdlog.h>
#include "zenoh.h"

using namespace uprotocol::uri;
using namespace uprotocol::v1;

ShmemMemoryAllocator::ShmemMemoryAllocator(const UUri &uri,
                                           const size_t &elemSize,
                                           const size_t &queueSize) noexcept {
    elemSize_ = elemSize;
    queueSize_ = queueSize;
    uriHash_ = std::hash<std::string>{}(LongUriSerializer::serialize(uri));
}
 
UCode ShmemMemoryAllocator::initialize() noexcept {

    ZenohSessionManagerConfig sessionConfig;

    if (UCode::OK != ZenohSessionManager::instance().init(sessionConfig)) {
        spdlog::error("zenohSessionManager::instance().init() failed");
        return UCode::UNAVAILABLE;
    }

    if (ZenohSessionManager::instance().getSession().has_value()) {
        session_ = ZenohSessionManager::instance().getSession().value();
    } else {
        return UCode::UNAVAILABLE;
    }

    manager_ = zc_shm_manager_new(z_loan(session_), 
                                  std::to_string(uriHash_).c_str(), 
                                  (queueSize_ * elemSize_));
    if (!z_check(manager_)) {
        spdlog::error("zc_shm_manager_new failed");
        return UCode::UNAVAILABLE;
    }

    return UCode::OK;
}
 
UCode ShmemMemoryAllocator::terminate() noexcept {

    zc_shm_manager_drop(&manager_);

    return UCode::OK;
}
         
uint8_t* ShmemMemoryAllocator::allocate() noexcept {

    zc_owned_shmbuf_t shmbuf = zc_shm_alloc(&manager_, 
                                            elemSize_);
    if (!z_check(shmbuf)) {
        spdlog::error("Failed to allocate a SHM buffer");
        return nullptr; 
    }                  

    zc_shmbuf_set_length(&shmbuf, 
                         elemSize_);

    auto ptr = zc_shmbuf_ptr(&shmbuf);
    auto key = std::hash<uint8_t*>{}(ptr);

    allocBufMap_[key] = shmbuf;

    ShmemMemoryMapper::instance().registerBuffer(shmbuf,
                                                 ptr);

    std::memset(ptr,
                0,
                elemSize_);
        
    return ptr;
}

zc_owned_payload_t ShmemMemoryAllocator::getShmemBuf(uint8_t *addr) noexcept
{
    auto key = std::hash<uint8_t*>{}(addr);

    if (allocBufMap_.find(key) == allocBufMap_.end()) {
        spdlog::error("addr was not found");
        //return UCode::INVALID_ARGUMENT;
    }
    
    zc_owned_shmbuf_t shmbuf = allocBufMap_[key];

    return zc_shmbuf_into_payload(z_move(shmbuf));
}

UCode ShmemMemoryAllocator::free(uint8_t *addr) noexcept {

    // need a way to check if the buffer was not sent yet 

    auto key = std::hash<uint8_t*>{}(addr);

    if (allocBufMap_.find(key) == allocBufMap_.end()) {
        spdlog::error("addr was not found");
        return UCode::INVALID_ARGUMENT;
    }

    zc_owned_shmbuf_t shmbuf = allocBufMap_[key];
  
  //  zc_shmbuf_drop(&shmbuf);

    ShmemMemoryMapper::instance().unRegisterBuffer(addr);

    return UCode::OK;
}

std::optional<std::vector<uint8_t*>> ShmemMemoryAllocator::getAddresses() noexcept {

    return std::nullopt;
}
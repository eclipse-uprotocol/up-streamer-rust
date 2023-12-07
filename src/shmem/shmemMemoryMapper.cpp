#include <shmemMemoryMapper.h>
#include <iostream>

using namespace std;

ShmemMemoryMapper& ShmemMemoryMapper::instance() noexcept {

    static ShmemMemoryMapper memoryMapper;

	return memoryMapper;
}

void ShmemMemoryMapper::registerBuffer(zc_owned_shmbuf_t shmemBuf, 
                                       uint8_t *addr) {
    auto key = std::hash<uint8_t*>{}(addr);

    allocBufMap_[key] = shmemBuf;
}

void ShmemMemoryMapper::unRegisterBuffer(uint8_t *addr) {

    auto key = std::hash<uint8_t*>{}(addr);

    if (allocBufMap_.find(key) == allocBufMap_.end()) {
        return;
    }

    allocBufMap_.erase(key);
}

zc_owned_shmbuf_t ShmemMemoryMapper::getShmemBuffer(uint8_t* addr) {

    auto key = std::hash<uint8_t*>{}(addr);

    if (allocBufMap_.find(key) != allocBufMap_.end()) {
        std::cout << "found key " << std::endl;
        return allocBufMap_[key];
    }
}
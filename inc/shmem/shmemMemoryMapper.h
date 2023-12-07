#ifndef _SHMEM_MEMORY_MAPPER_H_
#define _SHMEM_MEMORY_MAPPER_H_

#include <unordered_map>
#include <cstdint>
#include <zenoh.h>

using namespace std;

class ShmemMemoryMapper {

        public:
                static ShmemMemoryMapper& instance() noexcept;
                
                void registerBuffer(zc_owned_shmbuf_t shmemBuf, 
                                    uint8_t *addr);
                
                void unRegisterBuffer(uint8_t *addr);

                zc_owned_shmbuf_t getShmemBuffer(uint8_t* addr);
                
        private:
                std::unordered_map<size_t, zc_owned_shmbuf_t> allocBufMap_;
};


#endif /* _SHMEM_MEMORY_MAPPER_H_ */
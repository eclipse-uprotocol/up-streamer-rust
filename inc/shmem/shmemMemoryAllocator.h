#ifndef _SHMEM_MEMORY_ALLOCATOR_H_
#define _SHMEM_MEMORY_ALLOCATOR_H_

#include <optional>
#include <unordered_map>
#include <memoryAllocator.h>
#include <ustatus.pb.h>
#include <uprotocol/uri/datamodel/UUri.h>
#include <zenoh.h>

using namespace std;
using namespace uprotocol::uri;
using namespace uprotocol::v1;

class ShmemMemoryAllocator : public MemoryAllocator {

        public:

                ShmemMemoryAllocator(const UUri &uri,
                                     const size_t &elemSize,
                                     const size_t &queueSize) noexcept;
        
                /**
                * initialize memory handler
                * @return OK on success , error on failure
                */
                UCode initialize() noexcept;
        
                /**
                * terminate memory handler
                * @return OK on success , error on failure
                */
                UCode terminate() noexcept;
                
                /**
                * allocate shared memory buffer (ref counter will be increased)
                * @return pointer to a shared memory buffer, nullptr in case of failure
                */
                uint8_t* allocate() noexcept;
                
                /**
                * free shared memory buffer in case that the memory was not used  (ref counter will be decreased)
                * @param addr - address to free
                * @return OK on success , error on failure
                */
                UCode free(uint8_t *addr) noexcept;
      
                /**
                * ged addresses of all memory addresses (ref counter will not be increased)
                * @return vector on success , nullopt on failure
                */
                std::optional<std::vector<uint8_t*>> getAddresses() noexcept;

                zc_owned_payload_t getShmemBuf(uint8_t *addr) noexcept;
       
        private:

                zc_owned_shm_manager_t manager_;
                z_owned_session_t session_;
                size_t elemSize_;   
                size_t queueSize_;
                std::unordered_map<size_t, zc_owned_shmbuf_t> allocBufMap_;
                size_t uriHash_;
};

#endif /* _SHMEM_MEMORY_ALLOCATOR_H_*/
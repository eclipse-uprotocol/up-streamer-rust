project(demo LANGUAGES CXX)

include_directories(${CMAKE_CURRENT_SOURCE_DIR}/../../uprotocol-cpp/)

include_directories(${CMAKE_CURRENT_SOURCE_DIR}/../../uprotocol-cpp/uprotocol)

include_directories(${CMAKE_CURRENT_SOURCE_DIR}/../../uprotocol-cpp/uprotocol/uri/datamodel)

# need a correct way to handle the dependencie 
include_directories(${CMAKE_CURRENT_BINARY_DIR}/../../uprotocol-cpp/proto)

include_directories(${CMAKE_CURRENT_BINARY_DIR}/../uprotocol-core-api/src/main/proto/)

include_directories(${CMAKE_CURRENT_BINARY_DIR}/../../uprotocol-core-api/src/main/proto/)
include_directories(${CMAKE_CURRENT_SOURCE_DIR}/../inc)
include_directories(${CMAKE_CURRENT_SOURCE_DIR}/../inc/shmem)
include_directories(${CMAKE_CURRENT_SOURCE_DIR}/../inc/utils)

link_directories(${CMAKE_CURRENT_BINARY_DIR}/../lib)

find_package(spdlog REQUIRED)

add_executable(
    demo_server 
    src/main_server.cpp)

add_definitions(-DSPDLOG_FMT_EXTERNAL)

target_link_libraries(
    demo_server 
    ZenohUTransport 
    uprotocolsdk_shared 
    /usr/local/lib64/libzenohc.so
    fmt
    spdlog::spdlog)

add_executable(
    demo_server_shmem 
    src/main_server_shmem.cpp)

add_definitions(-DSPDLOG_FMT_EXTERNAL)

target_link_libraries(
    demo_server_shmem 
    ZenohUTransport 
    uprotocolsdk_shared 
    /usr/local/lib64/libzenohc.so
    fmt
    spdlog::spdlog)


add_executable(
    demo_client 
    src/main_client.cpp)

add_definitions(-DSPDLOG_FMT_EXTERNAL)

target_link_libraries(
    demo_client 
    ZenohUTransport 
    uprotocolsdk_shared 
    /usr/local/lib64/libzenohc.so
    fmt
    spdlog::spdlog)

add_executable(
    demo_rpc_server 
    src/main_rpc_server.cpp)

target_link_libraries(
    demo_rpc_server
    ZenohUTransport 
    uprotocolsdk_shared 
    /usr/local/lib64/libzenohc.so    
    fmt
    spdlog::spdlog)

add_executable(
    demo_rpc_client 
    src/main_rpc_client.cpp)

target_link_libraries(
    demo_rpc_client
    ZenohUTransport 
    uprotocolsdk_shared 
    /usr/local/lib64/libzenohc.so    
    fmt
    spdlog::spdlog)
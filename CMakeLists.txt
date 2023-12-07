project(ZenohUTransport LANGUAGES CXX)

include_directories(${CMAKE_CURRENT_SOURCE_DIR}/../uprotocol-cpp/)

include_directories(${CMAKE_CURRENT_SOURCE_DIR}/../uprotocol-cpp/uprotocol)

include_directories(${CMAKE_CURRENT_SOURCE_DIR}/../uprotocol-cpp/uprotocol/uri/datamodel)

# need a correct way to handle the dependencie 
include_directories(${CMAKE_CURRENT_BINARY_DIR}/../uprotocol-cpp/proto)

include_directories(${CMAKE_CURRENT_BINARY_DIR}/../uprotocol-core-api/src/main/proto/)

include_directories(inc)
include_directories(inc/shmem)
include_directories(inc/utils)
include_directories(/usr/local/include)

find_library(ZENOH_LIB zenohc)
find_package(spdlog REQUIRED)

add_definitions(-DSPDLOG_FMT_EXTERNAL)

add_library(
    ${PROJECT_NAME} SHARED 
    src/zenohUTransport.cpp
    src/zenohRpcClient.cpp
    src/zenohSessionManager.cpp
    src/messageBuilder.cpp
    src/messageParser.cpp
 #   src/uSubClient.cpp
    ${CMAKE_CURRENT_BINARY_DIR}/../uprotocol-core-api/src/main/proto/core/usubscription/v3/usubscription.pb.cc
    ${CMAKE_CURRENT_BINARY_DIR}/../uprotocol-core-api/src/main/proto/uri.pb.cc
    ${CMAKE_CURRENT_BINARY_DIR}/../uprotocol-core-api/src/main/proto/uuid.pb.cc
    ${CMAKE_CURRENT_BINARY_DIR}/../uprotocol-core-api/src/main/proto/file.pb.cc
    /usr/local/lib64/libzenohc.so)


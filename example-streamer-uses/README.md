# How to run "up-linux-streamer"
### Run following command from root directory of project:
     cargo run --bin up-linux-streamer -- --config up-linux-streamer/DEFAULT_CONFIG.json5  

# Examples
### Please note that these examples should be run in pairs. For instance, ue_service should be paired with me_client, and me_service should be paired with ue_client. Additionally, ensure that up-linux-streamer is running before executing these examples from root directory of project.

### How to run rpc examples on same local machine. 

```
    cargo run --bin ue_service
    cargo run --bin me_client
```

```
    cargo run --bin ue_client
    cargo run --bin me_service
```
### How to run publish subscribe examples on same local machine

```
    cargo run --bin ue_publisher
    cargo run --bin me_subscriber
```
```
    cargo run --bin ue_subscriber
    cargo run --bin me_publisher
```
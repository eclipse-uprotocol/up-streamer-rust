/********************************************************************************
 * Copyright (c) 2024 Contributors to the Eclipse Foundation
 *
 * See the NOTICE file(s) distributed with this work for additional
 * information regarding copyright ownership.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Apache License Version 2.0 which is available at
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

#![recursion_limit = "256"]

// TODO: Consider if we're ever likely to want to use this _not_ as a Zenoh plugin, in which
//  case the config module should be made pub and we should add validation on top of setting
//  its members
#[cfg(all(feature = "zenoh-transport", feature = "vsomeip-transport"))]
mod config;

#[cfg(all(feature = "zenoh-transport", feature = "vsomeip-transport"))]
pub mod plugin {

    const THREAD_NUM: usize = 10;

    use crate::config::{Config, HostTransport, SubscriptionProviderMode};
    use std::env;
    use std::sync::{
        atomic::{AtomicBool, Ordering::Relaxed},
        Arc, Mutex,
    };
    use std::time::Duration;
    use tracing::trace;
    use up_rust::core::usubscription::USubscription;
    use up_rust::{UTransport, UUri};
    use up_streamer::{Endpoint, UStreamer};
    use up_transport_vsomeip::UPTransportVsomeip;
    use up_transport_zenoh::UPTransportZenoh;
    use usubscription_static_file::USubscriptionStaticFile;
    use zenoh::internal::plugins::{RunningPluginTrait, ZenohPlugin};
    use zenoh::internal::runtime::DynamicRuntime;
    use zenoh_core::zlock;
    use zenoh_plugin_trait::{plugin_long_version, plugin_version, Plugin, PluginControl};
    use zenoh_result::{zerror, ZResult};
    use zenoh_util::ffi::JsonKeyValueMap;

    fn try_init_tracing_from_env() {
        let _ = tracing_subscriber::fmt::try_init();
    }

    // The struct implementing the ZenohPlugin and ZenohPlugin traits
    pub struct UpLinuxStreamerPlugin {}

    // declaration of the plugin's VTable for zenohd to find the plugin's functions to be called
    #[cfg(feature = "dynamic_plugin")]
    zenoh_plugin_trait::declare_plugin!(UpLinuxStreamerPlugin);

    impl ZenohPlugin for UpLinuxStreamerPlugin {}
    impl Plugin for UpLinuxStreamerPlugin {
        type StartArgs = DynamicRuntime;
        type Instance = zenoh::internal::plugins::RunningPlugin;

        // A mandatory const to define, in case of the plugin is built as a standalone executable
        const DEFAULT_NAME: &'static str = "up_linux_streamer";
        const PLUGIN_VERSION: &'static str = plugin_version!();
        const PLUGIN_LONG_VERSION: &'static str = plugin_long_version!();

        // The first operation called by zenohd on the plugin
        fn start(name: &str, runtime: &Self::StartArgs) -> ZResult<Self::Instance> {
            try_init_tracing_from_env();
            trace!("up-linux-streamer-plugin: start");

            let plugin_conf = runtime
                .get_config()
                .get_plugin_config(name)
                .map_err(|e| zerror!("Plugin `{}` configuration access error: {}", name, e))?;
            let config: Config = serde_json::from_value(plugin_conf)
                .map_err(|e| zerror!("Plugin `{}` configuration error: {}", name, e))?;
            trace!("loaded config: {config:?}");
            trace!("succeeded in reading plugin config");

            // a flag to end the plugin's loop when the plugin is removed from the config
            let flag = Arc::new(AtomicBool::new(true));
            // spawn the task running the plugin's loop
            trace!("up-linux-streamer-plugin: before spawning run");

            let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(THREAD_NUM)
                .enable_all()
                .build()
                .map_err(|err| zerror!("Unable to create runtime: {}", err))?;
            tokio_runtime.spawn({
                let runtime = runtime.clone();
                let config = config.clone();
                let flag = flag.clone();
                async move {
                    if let Err(err) = run(runtime, config, flag.clone()).await {
                        trace!("up-linux-streamer-plugin: run task exiting with error: {err}");
                        flag.store(false, Relaxed);
                    }
                }
            });
            trace!("up-linux-streamer-plugin: after spawning run");
            // return a RunningPlugin to zenohd
            trace!("up-linux-streamer-plugin: before creating RunningPlugin");
            let ret = Box::new(RunningPlugin(Arc::new(Mutex::new(RunningPluginInner {
                flag,
                tokio_runtime: Some(tokio_runtime),
            }))));
            trace!("up-linux-streamer-plugin: after creating RunningPlugin");

            Ok(ret)
        }
    }

    // An inner-state for the RunningPlugin
    struct RunningPluginInner {
        flag: Arc<AtomicBool>,
        tokio_runtime: Option<tokio::runtime::Runtime>,
    }
    // The RunningPlugin struct implementing the RunningPluginTrait trait
    #[derive(Clone)]
    struct RunningPlugin(Arc<Mutex<RunningPluginInner>>);

    impl PluginControl for RunningPlugin {}

    impl RunningPluginTrait for RunningPlugin {
        fn config_checker(
            &self,
            _path: &str,
            _old: &JsonKeyValueMap,
            _new: &JsonKeyValueMap,
        ) -> ZResult<Option<JsonKeyValueMap>> {
            // TODO: Learn more about how the config_checker is used
            Ok(None)
        }
    }

    // If the plugin is dropped, set the flag to false to end the loop
    impl Drop for RunningPlugin {
        fn drop(&mut self) {
            let mut inner = zlock!(self.0);
            inner.flag.store(false, Relaxed);
            if let Some(runtime) = inner.tokio_runtime.take() {
                runtime.shutdown_timeout(Duration::from_secs(2));
            }
        }
    }

    async fn run(runtime: DynamicRuntime, config: Config, flag: Arc<AtomicBool>) -> ZResult<()> {
        trace!("up-linux-streamer-plugin: inside of run");

        trace!("attempt to call something on the runtime");
        let timestamp_res = runtime.new_timestamp();
        trace!("called function on runtime: {timestamp_res:?}");

        let usubscription: Arc<dyn USubscription> = match config.usubscription_config.mode {
            SubscriptionProviderMode::StaticFile => Arc::new(USubscriptionStaticFile::new(
                config.usubscription_config.file_path.clone(),
            )),
            SubscriptionProviderMode::LiveUsubscription => {
                return Err(zerror!(
                        "live_usubscription mode is reserved in this phase; live runtime integration is deferred"
                    )
                    .into());
            }
        };

        let mut streamer = UStreamer::new(
            "up-linux-streamer",
            config.up_streamer_config.message_queue_size,
            usubscription,
        )
        .await
        .map_err(|error| zerror!("Failed to create uStreamer: {}", error))?;

        let streamer_uuri = UUri::try_from_parts(
            &config.streamer_uuri.authority,
            config.streamer_uuri.ue_id,
            config.streamer_uuri.ue_version_major,
            0,
        )
        .map_err(|err| zerror!("Unable to form streamer_uuri: {}", err))?;

        trace!("streamer_uuri: {streamer_uuri:#?}");
        let host_transport: Arc<dyn UTransport> = Arc::new(match config.host_config.transport {
            HostTransport::Zenoh => {
                let zenoh_session = zenoh::session::init(runtime.clone()).await.map_err(|err| {
                    zerror!("Unable to initialize Zenoh session from runtime: {}", err)
                })?;
                UPTransportZenoh::builder(config.streamer_uuri.authority.clone())
                    .map_err(|err| zerror!("Unable to create Zenoh transport builder: {}", err))?
                    .with_session(zenoh_session)
                    .build()
                    .map_err(|err| zerror!("Unable to initialize Zenoh UTransport: {}", err))?
            } // other host transports can be added here as they become available
        });

        let host_endpoint = Endpoint::new(
            "host_endpoint",
            &config.streamer_uuri.authority,
            host_transport.clone(),
        );

        if config.someip_config.enabled {
            let someip_config_file_abs_path = if config.someip_config.config_file.is_relative() {
                env::current_dir()
                    .map_err(|err| {
                        zerror!(
                            "Unable to resolve current_dir for someip config path: {}",
                            err
                        )
                    })?
                    .join(&config.someip_config.config_file)
            } else {
                config.someip_config.config_file.clone()
            };
            trace!("someip_config_file_abs_path: {someip_config_file_abs_path:?}");
            if !someip_config_file_abs_path.exists() {
                return Err(zerror!(
                    "The specified someip config_file doesn't exist: {:?}",
                    someip_config_file_abs_path
                )
                .into());
            }

            let host_uuri = UUri::try_from_parts(
                &config.streamer_uuri.authority,
                config
                    .someip_config
                    .default_someip_application_id_for_someip_subscriptions as u32,
                1,
                0,
            )
            .map_err(|err| zerror!("Unable to make host_uuri: {}", err))?;

            // There will be at most one vsomeip_transport, as there is a connection into device and a streamer
            let someip_transport: Arc<dyn UTransport> = Arc::new(
                UPTransportVsomeip::new_with_config(
                    host_uuri,
                    &config.someip_config.authority,
                    &someip_config_file_abs_path,
                    None,
                )
                .map_err(|err| zerror!("Unable to initialize vsomeip UTransport: {}", err))?,
            );

            let mechatronics_endpoint = Endpoint::new(
                "mechatronics_endpoint",
                &config.someip_config.authority,
                someip_transport.clone(),
            );
            let forwarding_res = streamer
                .add_route(mechatronics_endpoint.clone(), host_endpoint.clone())
                .await;

            if let Err(err) = forwarding_res {
                return Err(zerror!("Unable to add forwarding result: {:?}", err).into());
            }

            let forwarding_res = streamer
                .add_route(host_endpoint.clone(), mechatronics_endpoint.clone())
                .await;

            if let Err(err) = forwarding_res {
                return Err(zerror!("Unable to add forwarding result: {:?}", err).into());
            }
        }

        // Plugin's event loop, while the flag is true
        let mut counter = 1;
        while flag.load(Relaxed) {
            // TODO: Need to implement signaling to stop uStreamer

            tokio::time::sleep(Duration::from_millis(1000)).await;
            trace!("counter: {counter}");

            counter += 1;
        }

        Ok(())
    }
}

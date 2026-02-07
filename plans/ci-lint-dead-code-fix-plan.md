# Plan: Fix CI lint failure from feature-gated plugin config

## Goal

Fix the PR CI `Lint` failures caused by `dead_code` in `up-linux-streamer-plugin/src/config.rs` during the no-feature clippy pass.

## Steps

- [x] Add matching feature gate on `mod config;` in `up-linux-streamer-plugin/src/lib.rs` so it is compiled only when the plugin module is compiled.
- [x] Run `cargo clippy --all-targets -- -W warnings -D warnings` to confirm the dead-code errors are gone.
- [ ] Commit and push the fix to the current PR branch.

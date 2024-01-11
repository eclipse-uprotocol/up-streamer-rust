#!/bin/sh

cargo fmt -- --check
cargo clippy --all-targets -- -W warnings -D warnings
cargo doc -p up-streamer --no-deps

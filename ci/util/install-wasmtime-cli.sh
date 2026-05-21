#!/bin/sh

rustup toolchain install 1.91.0
rustup run 1.91.0 cargo install --version 33.0.0 wasmtime-cli

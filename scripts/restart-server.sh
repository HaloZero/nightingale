#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

echo "Building server (release)..."
cargo build -p server --release

echo "Stopping any running instance..."
pkill -f "target/release/server" 2>/dev/null || true
sleep 1

# No NIGHTINGALE_DATA_PATH set on purpose -- it reads the default
# ~/.nightingale/config.json, which already points at the real data
# directory via its own internal "data_path" field.
nohup target/release/server > ~/.nightingale/nightingale.log 2>&1 &
disown

echo "Server restarted (PID $!), logging to ~/.nightingale/nightingale.log"

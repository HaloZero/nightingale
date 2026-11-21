#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

# cargo build -p server never invokes pnpm/vite itself -- it just embeds
# whatever's already sitting in client/dist/ at compile time (rust-embed,
# see client/src-server/src/static_files.rs). Skipping this step silently
# ships a stale frontend with no build error to warn you.
echo "Building frontend..."
(cd client && pnpm build)

echo "Building server (release)..."
cargo build -p server --release

echo "Build complete."

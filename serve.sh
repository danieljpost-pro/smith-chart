#!/usr/bin/env bash
# Serve the app locally (wasm modules cannot be loaded from file:// URLs).
set -euo pipefail
cd "$(dirname "$0")/www"
PORT="${1:-8080}"
echo "http://localhost:${PORT}/"
exec python3 -m http.server "$PORT"

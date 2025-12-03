#!/bin/bash
# Start the Tauri uploader in dev mode pointing to localhost Academy API
# Usage: ./scripts/dev-local.sh

cd "$(dirname "$0")/.." || exit 1

echo "Starting Ladder Legends Uploader in dev mode..."
echo "API Host: http://localhost:3000"
echo ""

# Set both env vars:
# - VITE_API_HOST: Used by Vite at build time (inlined into JS bundle)
# - LADDER_LEGENDS_API_HOST: Used by Rust backend and runtime window injection
VITE_API_HOST=http://localhost:3000 LADDER_LEGENDS_API_HOST=http://localhost:3000 cargo tauri dev

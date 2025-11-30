#!/bin/bash
# Start the Tauri uploader in dev mode pointing to localhost Academy API
# Usage: ./scripts/dev-local.sh

cd "$(dirname "$0")/.." || exit 1

echo "Starting Ladder Legends Uploader in dev mode..."
echo "API Host: http://localhost:3000"
echo ""

LADDER_LEGENDS_API_HOST=http://localhost:3000 cargo tauri dev

#!/bin/bash
# Start the Tauri uploader in dev mode pointing to localhost Academy API
# Usage: ./scripts/dev-local.sh [--prod]
#   --prod  Use production API instead of localhost

cd "$(dirname "$0")/.." || exit 1

# Default to localhost, --prod for production
API_HOST="http://localhost:3000"
if [ "$1" = "--prod" ]; then
  API_HOST="https://www.ladderlegendsacademy.com"
fi

# Set up dev replay folder with sample replays (no SC2 required)
DEV_REPLAYS="$HOME/.ladder-legends-dev/replays"
if [ ! -d "$DEV_REPLAYS" ]; then
  echo "Setting up dev replay folder at $DEV_REPLAYS..."
  mkdir -p "$DEV_REPLAYS"

  # Copy sample replays from sc2-replay-analyzer test fixtures
  FIXTURES="../sc2-replay-analyzer/tests/fixtures"
  E2E_REPLAYS="../sc2-replay-analyzer/tests/e2e/replays"

  if [ -d "$FIXTURES" ]; then
    cp "$FIXTURES"/*.SC2Replay "$DEV_REPLAYS/" 2>/dev/null
  fi
  if [ -d "$E2E_REPLAYS" ]; then
    # Copy a few e2e replays (not all — just enough to test)
    ls "$E2E_REPLAYS"/*.SC2Replay 2>/dev/null | head -5 | xargs -I{} cp {} "$DEV_REPLAYS/"
  fi

  REPLAY_COUNT=$(ls "$DEV_REPLAYS"/*.SC2Replay 2>/dev/null | wc -l | tr -d ' ')
  echo "Copied $REPLAY_COUNT replay(s) to dev folder"
else
  REPLAY_COUNT=$(ls "$DEV_REPLAYS"/*.SC2Replay 2>/dev/null | wc -l | tr -d ' ')
  echo "Dev replay folder exists ($REPLAY_COUNT replays)"
fi

echo ""
echo "Starting Ladder Legends Uploader in dev mode..."
echo "  API Host: $API_HOST"
echo "  Replay Folder: $DEV_REPLAYS"
echo ""

# Set env vars:
# - VITE_API_HOST: Used by Vite at build time (inlined into JS bundle)
# - LADDER_LEGENDS_API_HOST: Used by Rust backend and runtime window injection
# - DEV_REPLAY_FOLDER: Bypasses SC2 folder detection in detect_replay_folders command
VITE_API_HOST="$API_HOST" \
LADDER_LEGENDS_API_HOST="$API_HOST" \
DEV_REPLAY_FOLDER="$DEV_REPLAYS" \
cargo tauri dev

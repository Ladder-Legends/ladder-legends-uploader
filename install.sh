#!/bin/bash

# Build and install Ladder Legends Uploader to /Applications
# This script is for LOCAL DEVELOPMENT ONLY - builds with localhost API
# IMPORTANT: This script clears all app data/settings for clean testing

set -e  # Exit on error

APP_ID="com.ladderlegends.uploader"
APP_NAME="Ladder Legends Uploader"

echo "=========================================="
echo "Installing $APP_NAME (Development Mode)"
echo "=========================================="
echo ""

# Step 1: Kill running app process
echo "1. Stopping any running instances..."
pkill -x "ladder-legends-uploader" 2>/dev/null || true
pkill -f "$APP_NAME" 2>/dev/null || true
sleep 1

# Step 2: Clear all app data and settings
echo "2. Clearing app data and settings..."

# Application Support (main app data)
if [ -d "$HOME/Library/Application Support/$APP_ID" ]; then
    echo "   - Removing Application Support..."
    rm -rf "$HOME/Library/Application Support/$APP_ID"
fi

# Caches
if [ -d "$HOME/Library/Caches/$APP_ID" ]; then
    echo "   - Removing Caches..."
    rm -rf "$HOME/Library/Caches/$APP_ID"
fi

# WebKit storage (localStorage, cookies, etc)
if [ -d "$HOME/Library/WebKit/$APP_ID" ]; then
    echo "   - Removing WebKit storage..."
    rm -rf "$HOME/Library/WebKit/$APP_ID"
fi

# Preferences plist
if [ -f "$HOME/Library/Preferences/$APP_ID.plist" ]; then
    echo "   - Removing Preferences..."
    rm -f "$HOME/Library/Preferences/$APP_ID.plist"
fi

# Remove from Login Items (macOS manages these, but try to clean up)
# Note: macOS 13+ stores these in a binary database, older versions in plist
osascript -e "tell application \"System Events\" to delete every login item where name is \"$APP_NAME\"" 2>/dev/null || true

echo "   ✓ App data cleared"
echo ""

# Step 3: Build app
echo "3. Building release version (localhost API)..."
LADDER_LEGENDS_API_HOST=http://localhost:3000 cargo tauri build --bundles app

echo "   ✓ Build complete"
echo ""

# Step 4: Install to /Applications
echo "4. Installing to /Applications..."
APP_PATH="src-tauri/target/release/bundle/macos/$APP_NAME.app"

if [ ! -d "$APP_PATH" ]; then
    echo "   ✗ Error: App bundle not found at $APP_PATH"
    exit 1
fi

# Remove old version if exists
if [ -d "/Applications/$APP_NAME.app" ]; then
    echo "   - Removing old version..."
    rm -rf "/Applications/$APP_NAME.app"
fi

# Copy new version
cp -R "$APP_PATH" /Applications/

# Update Spotlight index
echo "   - Updating Spotlight index..."
sudo mdutil -E /Applications 2>/dev/null || true

echo "   ✓ Installed to /Applications"
echo ""

# Step 5: Fix LaunchAgent path if it exists
echo "5. Updating LaunchAgent path (if autostart was enabled)..."
PLIST_FILE="$HOME/Library/LaunchAgents/$APP_NAME.plist"
if [ -f "$PLIST_FILE" ]; then
    echo "   - LaunchAgent found, unloading old path..."
    launchctl unload "$PLIST_FILE" 2>/dev/null || true

    echo "   - Updating to /Applications path..."
    # The app will recreate this with the correct path when it runs
    # For now, just remove the old one so it doesn't point to build dir
    rm -f "$PLIST_FILE"

    echo "   ✓ LaunchAgent cleared (will be recreated with correct path on next run)"
else
    echo "   - No LaunchAgent found (autostart not previously enabled)"
fi
echo ""

echo "=========================================="
echo "✓ Installation complete!"
echo "=========================================="
echo ""
echo "The app has been installed with:"
echo "  • Fresh settings (all data cleared)"
echo "  • Localhost API (http://localhost:3000)"
echo "  • Login Items ready to configure"
echo ""
echo "To open the app:"
echo "  • Press Cmd+Space and type '$APP_NAME'"
echo "  • Or open from /Applications"
echo ""
echo "Note: First launch may show security warning."
echo "      Right-click → Open to bypass Gatekeeper."
echo ""

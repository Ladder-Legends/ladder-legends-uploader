#!/bin/bash

# Build production release for distribution
# This builds with the production API URL (ladderlegendsacademy.com)
# DO NOT use this for local development - use ./install.sh instead

echo "========================================="
echo "Building PRODUCTION release"
echo "API URL: https://ladderlegendsacademy.com"
echo "========================================="
echo ""

# Unset LADDER_LEGENDS_API_HOST to ensure production URL
unset LADDER_LEGENDS_API_HOST

echo "Building all bundle formats..."
cargo tauri build

if [ $? -ne 0 ]; then
    echo "Build failed!"
    exit 1
fi

echo ""
echo "========================================="
echo "✓ Production build complete!"
echo "========================================="
echo ""
echo "Distributable files created:"
echo ""
ls -lh src-tauri/target/release/bundle/dmg/*.dmg 2>/dev/null
echo ""
echo "Upload the DMG to GitHub Releases for distribution"
echo ""

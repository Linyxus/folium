#!/bin/bash
set -euo pipefail

if [ $# -ne 1 ]; then
    echo "Usage: ./release.sh <version>"
    echo "Example: ./release.sh v0.1.0"
    exit 1
fi

VERSION="$1"
APP_NAME="Folium"
DMG_PATH="target/release/${APP_NAME}.dmg"

echo "==> Releasing ${APP_NAME} ${VERSION}"

# Ensure working tree is clean.
if [ -n "$(git status --porcelain)" ]; then
    echo "Error: working tree is dirty. Commit or stash changes first."
    exit 1
fi

# Tag.
echo "==> Tagging ${VERSION}..."
git tag "${VERSION}"
git push origin "${VERSION}"

# Build app + DMG.
echo "==> Building..."
./build.sh --dmg

# Publish GitHub release.
echo "==> Creating GitHub release..."
gh release create "${VERSION}" "${DMG_PATH}" \
    --title "${APP_NAME} ${VERSION}" \
    --generate-notes

echo "==> Done: ${APP_NAME} ${VERSION} published."

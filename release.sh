#!/bin/bash
set -euo pipefail

FORCE=false
RELEASE_ONLY=false
VERSION=""

for arg in "$@"; do
    case "$arg" in
        --force) FORCE=true ;;
        --release-only) RELEASE_ONLY=true ;;
        *) VERSION="$arg" ;;
    esac
done

if [ -z "$VERSION" ]; then
    echo "Usage: ./release.sh [--force] [--release-only] <version>"
    echo "Example: ./release.sh v0.1.0"
    exit 1
fi

APP_NAME="Folium"
DMG_PATH="target/release/${APP_NAME}.dmg"

# Auto-detect the GitHub remote.
GH_REMOTE=$(git remote -v | grep 'github\.com' | head -1 | awk '{print $1}')
if [ -z "$GH_REMOTE" ]; then
    echo "Error: no GitHub remote found."
    exit 1
fi

echo "==> Releasing ${APP_NAME} ${VERSION}"

if [ "$RELEASE_ONLY" = true ]; then
    # Verify tag exists locally.
    if ! git rev-parse "${VERSION}" >/dev/null 2>&1; then
        echo "Error: tag ${VERSION} does not exist locally."
        exit 1
    fi
    # Verify tag is pushed to remote.
    if ! git ls-remote --tags "$GH_REMOTE" "${VERSION}" | grep -q "${VERSION}"; then
        echo "Error: tag ${VERSION} is not pushed to remote."
        exit 1
    fi
    # Verify DMG exists.
    if [ ! -f "${DMG_PATH}" ]; then
        echo "Error: ${DMG_PATH} not found. Run ./build.sh --dmg first."
        exit 1
    fi
else
    # Ensure working tree is clean (unless --force).
    if [ "$FORCE" = false ] && [ -n "$(git status --porcelain)" ]; then
        echo "Error: working tree is dirty. Commit or stash changes first, or use --force."
        exit 1
    fi

    # Tag.
    echo "==> Tagging ${VERSION}..."
    git tag "${VERSION}"
    git push "$GH_REMOTE" "${VERSION}"

    # Build app + DMG.
    echo "==> Building..."
    ./build.sh --dmg
fi

# Publish GitHub release.
echo "==> Creating GitHub release..."
gh release create "${VERSION}" "${DMG_PATH}" \
    --title "${APP_NAME} ${VERSION}" \
    --generate-notes

echo "==> Done: ${APP_NAME} ${VERSION} published."

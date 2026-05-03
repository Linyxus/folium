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
CASK_PATH="Casks/folium.rb"
# Cask version field is unprefixed; URL re-adds the leading "v".
CASK_VERSION="${VERSION#v}"

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

    # Build app + DMG. Done before tagging so the tag commit can include
    # the matching cask update.
    echo "==> Building..."
    ./build.sh --dmg

    # Update cask with new version + sha256.
    SHA256=$(shasum -a 256 "${DMG_PATH}" | awk '{print $1}')
    echo "==> Updating ${CASK_PATH} (version=${CASK_VERSION}, sha256=${SHA256})..."
    sed -i '' -E "s|^  version \"[^\"]*\"|  version \"${CASK_VERSION}\"|" "${CASK_PATH}"
    sed -i '' -E "s|^  sha256 \"[^\"]*\"|  sha256 \"${SHA256}\"|" "${CASK_PATH}"

    if ! git diff --quiet -- "${CASK_PATH}"; then
        echo "==> Committing cask update..."
        git add "${CASK_PATH}"
        git commit -m "Release ${VERSION}"
    fi

    # Tag, then push branch + tag together.
    echo "==> Tagging ${VERSION}..."
    git tag "${VERSION}"
    git push "$GH_REMOTE" HEAD
    git push "$GH_REMOTE" "${VERSION}"
fi

# Publish GitHub release.
echo "==> Creating GitHub release..."
gh release create "${VERSION}" "${DMG_PATH}" \
    --title "${APP_NAME} ${VERSION}" \
    --generate-notes

echo "==> Done: ${APP_NAME} ${VERSION} published."

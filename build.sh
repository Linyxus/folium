#!/bin/bash
set -euo pipefail

APP_NAME="Folium"
BUNDLE_ID="com.linyxus.folium"
VERSION="0.1.0"
BINARY_NAME="folium"

APP_DIR="target/release/${APP_NAME}.app"
CONTENTS_DIR="${APP_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"

echo "==> Building release binary..."
cargo build --release

echo "==> Creating app bundle..."
rm -rf "${APP_DIR}"
mkdir -p "${MACOS_DIR}"
mkdir -p "${RESOURCES_DIR}"

cp "target/release/${BINARY_NAME}" "${MACOS_DIR}/${BINARY_NAME}"

if [ -f "assets/AppIcon.icns" ]; then
    cp "assets/AppIcon.icns" "${RESOURCES_DIR}/AppIcon.icns"
    echo "    Copied app icon"
fi

cat > "${CONTENTS_DIR}/Info.plist" << 'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Folium</string>
    <key>CFBundleDisplayName</key>
    <string>Folium</string>
    <key>CFBundleIdentifier</key>
    <string>com.linyxus.folium</string>
    <key>CFBundleVersion</key>
    <string>0.1.0</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleExecutable</key>
    <string>folium</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key>
            <string>PDF Document</string>
            <key>CFBundleTypeRole</key>
            <string>Viewer</string>
            <key>LSItemContentTypes</key>
            <array>
                <string>com.adobe.pdf</string>
            </array>
            <key>LSHandlerRank</key>
            <string>Alternate</string>
        </dict>
    </array>
</dict>
</plist>
PLIST

echo "    Created Info.plist"
echo "==> App bundle: ${APP_DIR}"

if [ "${1:-}" = "--dmg" ]; then
    DMG_PATH="target/release/${APP_NAME}.dmg"
    STAGING="target/release/dmg-staging"
    echo "==> Staging DMG contents..."
    rm -rf "${STAGING}"
    mkdir -p "${STAGING}"
    cp -R "${APP_DIR}" "${STAGING}/"
    ln -s /Applications "${STAGING}/Applications"

    echo "==> Creating DMG..."
    rm -f "${DMG_PATH}"

    # Check for create-dmg (brew install create-dmg) for a polished result.
    if command -v create-dmg &>/dev/null; then
        create-dmg \
            --volname "${APP_NAME}" \
            --window-pos 200 120 \
            --window-size 660 400 \
            --icon-size 128 \
            --icon "${APP_NAME}.app" 180 190 \
            --icon "Applications" 480 190 \
            --hide-extension "${APP_NAME}.app" \
            --app-drop-link 480 190 \
            --no-internet-enable \
            "${DMG_PATH}" \
            "${STAGING}"
        # create-dmg exits 2 if it couldn't set a custom background (non-fatal).
        true
    else
        echo "    (install 'create-dmg' for a prettier DMG: brew install create-dmg)"
        hdiutil create -volname "${APP_NAME}" \
            -srcfolder "${STAGING}" \
            -ov -format UDZO \
            "${DMG_PATH}"
    fi
    rm -rf "${STAGING}"
    echo "==> DMG: ${DMG_PATH}"
fi

echo "==> Done. Run with: open ${APP_DIR}"

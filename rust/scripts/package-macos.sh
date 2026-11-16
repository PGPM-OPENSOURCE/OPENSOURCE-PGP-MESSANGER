#!/bin/sh
# Builds PGPM.app and a DMG from the Rust sources (macOS), from the rust/ folder:
#   scripts/package-macos.sh            -> dist/PGPM.app, dist/PGPM-macOS.dmg
# The app is signed ad hoc (no Developer ID): on first launch from a download, macOS asks to
# confirm opening it (right-click > Open). It keeps the Python build's bundle identifier, so
# it replaces that app in place.
set -eu
cd "$(dirname "$0")/.."

cargo build -p pgpm --release

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
OUT=dist
APP="$OUT/PGPM.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/pgpm "$APP/Contents/MacOS/pgpm"
cp ../assets/icon.icns "$APP/Contents/Resources/AppIcon.icns"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleDisplayName</key><string>PGPM</string>
  <key>CFBundleExecutable</key><string>pgpm</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundleIdentifier</key><string>PGPM</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>PGPM</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
  <key>LSMinimumSystemVersion</key><string>10.15</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>GPL-3.0</string>
</dict>
</plist>
PLIST
codesign --force --deep --sign - "$APP"
codesign --verify --strict "$APP"

STAGE=$(mktemp -d)
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
rm -f "$OUT/PGPM-macOS.dmg"
hdiutil create -quiet -volname "PGPM" -srcfolder "$STAGE" -ov -format UDZO "$OUT/PGPM-macOS.dmg"
rm -rf "$STAGE"
echo "built: $APP"
echo "built: $OUT/PGPM-macOS.dmg"

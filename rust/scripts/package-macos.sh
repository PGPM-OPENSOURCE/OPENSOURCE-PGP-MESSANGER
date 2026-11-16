#!/bin/sh
# Builds PGPM.app, a DMG and the setup package from the Rust sources (macOS), from the
# rust/ folder:
#   scripts/package-macos.sh   -> dist/PGPM.app, dist/PGPM-macOS.dmg, dist/PGPM-Setup.pkg
# PGPM-Setup.pkg is the counterpart of the Windows PGPM-Setup.exe: PGPM in Applications,
# GnuPG when the Mac has none (GPG Suite, downloaded from gpgtools.org while it installs),
# and an optional desktop icon (installer/).
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
# the setup package: one component per choice (installer/distribution.xml)
WORK=$(mktemp -d)
mkdir -p "$WORK/root" "$WORK/pkgs" "$WORK/resources"
ditto "$APP" "$WORK/root/PGPM.app"
cat > "$WORK/component.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<array>
  <dict>
    <key>RootRelativeBundlePath</key><string>PGPM.app</string>
    <key>BundleIsRelocatable</key><false/>
    <key>BundleIsVersionChecked</key><false/>
    <key>BundleHasStrictIdentifier</key><true/>
    <key>BundleOverwriteAction</key><string>upgrade</string>
  </dict>
</array>
</plist>
PLIST
pkgbuild --quiet --root "$WORK/root" --component-plist "$WORK/component.plist" --identifier PGPM \
    --version "$VERSION" --install-location /Applications --scripts installer/scripts/app "$WORK/pkgs/PGPM.pkg"
pkgbuild --quiet --nopayload --identifier PGPM.gnupg --version "$VERSION" \
    --scripts installer/scripts/gnupg "$WORK/pkgs/GnuPG.pkg"
pkgbuild --quiet --nopayload --identifier PGPM.desktop --version "$VERSION" \
    --scripts installer/scripts/desktop "$WORK/pkgs/Desktop.pkg"
for f in installer/resources/*.html; do
    sed "s/@VERSION@/$VERSION/g" "$f" > "$WORK/resources/$(basename "$f")"
done
sed "s/@VERSION@/$VERSION/g" installer/distribution.xml > "$WORK/distribution.xml"
rm -f "$OUT/PGPM-Setup.pkg"
productbuild --quiet --distribution "$WORK/distribution.xml" --resources "$WORK/resources" \
    --package-path "$WORK/pkgs" "$OUT/PGPM-Setup.pkg"
rm -rf "$WORK"
echo "built: $APP"
echo "built: $OUT/PGPM-macOS.dmg"
echo "built: $OUT/PGPM-Setup.pkg"

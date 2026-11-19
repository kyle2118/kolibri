#!/bin/sh
# Build Kolibri.app (macOS) with the app icon.
# Usage: scripts/bundle-macos.sh        -> target/release/Kolibri.app
set -eu
cd "$(dirname "$0")/.."

cargo build --release

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
app=target/release/Kolibri.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/kolibri "$app/Contents/MacOS/kolibri"
cp assets/kolibri.icns "$app/Contents/Resources/kolibri.icns"

cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Kolibri</string>
  <key>CFBundleDisplayName</key><string>Kolibri</string>
  <key>CFBundleIdentifier</key><string>io.github.kyle2118.kolibri</string>
  <key>CFBundleExecutable</key><string>kolibri</string>
  <key>CFBundleIconFile</key><string>kolibri</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF

echo "Built $app"

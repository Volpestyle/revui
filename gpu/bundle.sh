#!/bin/sh
# Builds revui.app and a disk image: the interface, and the revui it talks to, in one folder.
set -eu
cd "$(dirname "$0")"
(cd .. && cargo build --release --locked)
cargo build --release --locked
app=target/revui.app
rm -rf "$app" && mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/revui-gpu "$app/Contents/MacOS/revui-app"
cp ../target/release/revui "$app/Contents/MacOS/revui"
cp icon.icns "$app/Contents/Resources/icon.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>revui</string>
  <key>CFBundleDisplayName</key><string>revui</string>
  <key>CFBundleIdentifier</key><string>com.volpestyle.revui</string>
  <key>CFBundleExecutable</key><string>revui-app</string>
  <key>CFBundleIconFile</key><string>icon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>0.1.0</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
codesign --force --deep --sign - "$app"
rm -f target/revui.dmg
hdiutil create -quiet -volname revui -srcfolder "$app" -ov -format UDZO target/revui.dmg
echo "$app"

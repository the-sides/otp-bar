#!/bin/zsh
# Build OTP Bar and install it as ~/Applications/OTP Bar.app (menu bar only, no Dock icon).
set -euo pipefail
cd "${0:A:h}"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
cargo build --release

APP="$HOME/Applications/OTP Bar.app"
pkill -x otp-bar 2>/dev/null && while pgrep -qx otp-bar; do sleep 0.1; done || true
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp target/release/otp-bar "$APP/Contents/MacOS/otp-bar"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>OTP Bar</string>
  <key>CFBundleIdentifier</key><string>local.otp-bar</string>
  <key>CFBundleExecutable</key><string>otp-bar</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>LSUIElement</key><true/>
</dict></plist>
PLIST
codesign --force --sign - "$APP"
open "$APP"
echo "Installed and launched: $APP"

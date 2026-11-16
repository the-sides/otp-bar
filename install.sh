#!/bin/zsh
# Build OTP Bar, install it, and start it now and at login (no Dock icon).
set -euo pipefail
cd "${0:A:h}"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
cargo build --release

APP="$HOME/Applications/OTP Bar.app"
LABEL="local.otp-bar"
AGENT="$HOME/Library/LaunchAgents/$LABEL.plist"
DOMAIN="gui/$(id -u)"
launchctl bootout "$DOMAIN/$LABEL" 2>/dev/null || true
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

# RunAtLoad launches once per login; quitting the app leaves it quit.
mkdir -p "$HOME/Library/LaunchAgents"
AGENT_TMP="$(mktemp "$HOME/Library/LaunchAgents/$LABEL.XXXXXX")"
trap 'rm -f "$AGENT_TMP"' EXIT
plutil -create xml1 "$AGENT_TMP"
plutil -insert Label -string "$LABEL" "$AGENT_TMP"
plutil -insert ProgramArguments -json '[]' "$AGENT_TMP"
plutil -insert ProgramArguments.0 -string "$APP/Contents/MacOS/otp-bar" "$AGENT_TMP"
plutil -insert LimitLoadToSessionType -string Aqua "$AGENT_TMP"
plutil -insert RunAtLoad -bool true "$AGENT_TMP"
plutil -lint "$AGENT_TMP"
chmod 644 "$AGENT_TMP"
mv "$AGENT_TMP" "$AGENT"
launchctl enable "$DOMAIN/$LABEL"
launchctl bootstrap "$DOMAIN" "$AGENT"
echo "Installed, launched, and enabled at login: $APP"

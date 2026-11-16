# OTP Bar

A tiny macOS menu bar app (Rust, native AppKit) that turns a pasted secret key into a one-time access code (TOTP), instantly on your clipboard.

- Click the key icon: a text box opens, already focused.
- Press ⌘V with a secret key on your clipboard: the 6-digit code replaces it on the clipboard right away. No Enter needed.
- The popover shows the code and how many seconds it has left. When it rolls over, click the code to copy the new one.
- Accepts bare base32 keys (any case, with spaces or dashes) and `otpauth://totp/...` links, including custom digits, period and SHA-256/512.
- The secret is kept in memory only, never written to disk, and cleared from the text box after a paste. The copied code is marked concealed so clipboard managers skip it.

## Install

    ./install.sh     # builds, installs ~/Applications/OTP Bar.app, enables login startup, and launches

The installer registers a per-user LaunchAgent (`local.otp-bar`) that starts the
app once when you log in. Quit stays quit until you open it again or next log in.
Re-running the installer updates the app and its login configuration.

To disable login startup:

    launchctl disable "gui/$(id -u)/local.otp-bar"

To enable it again:

    launchctl enable "gui/$(id -u)/local.otp-bar"

To stop it now:

    launchctl bootout "gui/$(id -u)/local.otp-bar"

The installed app can also be opened from `~/Applications/OTP Bar.app`.

## Command line

    otp-bar --code JBSWY3DPEHPK3PXP    # prints the current code

## Test

    cargo test       # includes the RFC 6238 test vectors

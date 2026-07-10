# Releasing

Voice Watch releases should include prebuilt Windows binaries so regular users
do not need Rust, Node, or any developer tooling.

## Release artifacts

Each public release should include:

- `VoiceWatch-<version>-Setup.exe`
- `voice-watch-windows-x64.exe`
- `voice-watch-extension-<version>.zip`
- `voice-watch-connector-chrome-<version>.zip`
- `voice-watch-connector-edge-<version>.zip`
- `voice-watch-connector-firefox-<version>.zip`
- `voice-watch-connector-firefox-source-<version>.zip`
- `SHA256SUMS.txt`

The installer uses Inno Setup and installs Voice Watch per user under:

```text
%LOCALAPPDATA%\Programs\Voice Watch
```

It also offers an optional desktop shortcut during setup.

## Build locally

Install:

- Rust stable
- Node.js
- Inno Setup 6

Then run:

```powershell
cargo test
cargo clippy --all-targets -- -D warnings
.\scripts\test-update-handoff.ps1
.\scripts\package-extensions.ps1
.\scripts\build-installer.ps1
```

The extension packaging script also creates the Mozilla AMO source-review ZIP at
`dist\voice-watch-connector-firefox-source-<version>.zip`.

If the production browser extension ID is known, compile the installer with
native messaging registration enabled:

```powershell
.\scripts\build-installer.ps1 -ExtensionId "your-extension-id"
```

Without an extension ID, the installer still installs the desktop app and
bundles the extension files, but native messaging registration must be completed
after loading or publishing the extension.

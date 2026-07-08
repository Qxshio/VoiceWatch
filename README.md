# Voice Watch

Voice Watch: A privacy-first lightweight Windows tray app that tracks Roblox
voice chat suspension timers and notifies you when VC is restored.

This project is not affiliated with Roblox.

## Privacy model

Voice Watch is designed around a hard boundary: the desktop app never needs
Roblox cookies.

Voice Watch does not read your browser cookie database.  
Voice Watch does not extract `.ROBLOSECURITY`.  
Voice Watch does not upload Roblox cookies or tokens.  
The browser extension uses your existing browser login session to request VC
status from Roblox and sends only sanitized status data to the local desktop
app.

The extension sends only these fields:

- `isVoiceEnabled`
- `isUserOptIn`
- `isUserEligible`
- `isBanned`
- `banReason`
- `bannedUntilMs`
- `denialReason`
- `checkedAt`

See [docs/PRIVACY.md](docs/PRIVACY.md) for the full model.

## Current prototype status

The first prototype includes:

- Rust desktop project structure with clean slices for state, settings, tray,
  native messaging, process detection, countdown logic, Roblox log parsing, and
  rejoin behavior.
- Native messaging host mode with Chrome/Edge framing and `hello` /
  `hello_ack` support.
- Manifest V3 browser extension that connects to the native host.
- Real extension fetch to `https://voice.roblox.com/v1/settings` using
  `credentials: "include"` and no cookie permission.
- Sanitized voice status conversion, including `bannedUntilMs`.
- Local countdown anchoring that is resilient to system clock changes after the
  status fetch.
- A basic tray menu and a native dialog notification fallback for the restore
  overlay.
- Development scripts for registering and unregistering the native messaging
  host.

The polished Medal-style overlay, full local IPC bridge, settings UI, and
packaged installer are planned follow-up work.

## Requirements

- Windows 10 or newer

Regular users should download the installer from GitHub Releases. Rust is only
needed for contributors.

Contributor requirements:

- Rust stable toolchain
- Chrome or Edge for extension development
- Inno Setup 6 for building the Windows installer

Install Rust from <https://rustup.rs/> if `cargo` is not available.

## Run the desktop app

```powershell
cargo run
```

Useful development commands:

```powershell
cargo run -- --simulate-suspension 30
cargo run -- --native-host
cargo run -- --print-config-path
```

The settings file is created under `%APPDATA%\Voice Watch\settings.json`.

## Install from release

Download `VoiceWatch-<version>-Setup.exe` from
<https://github.com/Qxshio/VoiceWatch/releases>.

The installer:

- installs Voice Watch for the current Windows user,
- adds Start menu shortcuts,
- optionally creates a desktop shortcut,
- installs bundled extension/setup files for reference.

Prebuilt standalone binaries are also attached to each release for users who
prefer not to run an installer.

Default settings:

```json
{
  "pollIntervalSeconds": 10,
  "onlyPollWhenRobloxRunning": true,
  "showOverlay": true,
  "playSoundOnRestore": true,
  "overlayPosition": "top-right",
  "launchOnStartup": false
}
```

`pollIntervalSeconds` is clamped to 10-300 seconds.

## Load the extension in Chrome or Edge

1. Build the desktop app:

   ```powershell
   cargo build --release
   ```

2. Open `chrome://extensions` or `edge://extensions`.
3. Enable developer mode.
4. Choose **Load unpacked** and select the `extension/` folder.
5. Copy the generated extension ID.
6. Register the native messaging host:

   ```powershell
   .\scripts\register-native-host.ps1 -ExtensionId "your-extension-id" -Browser Both
   ```

7. Click the Voice Watch extension and choose **Connect**.

To remove the native messaging registration:

```powershell
.\scripts\unregister-native-host.ps1 -Browser Both -RemoveManifest
```

## Rejoin last server

Rejoining is always user-clicked. Voice Watch never auto-rejoins.

The app tries to infer the last Roblox place and server ID from local Roblox
logs. This is best-effort:

- If `placeId` and `gameInstanceId` are available, the rejoin action opens a
  Roblox deep link.
- If only `placeId` is available, it opens the Roblox experience page.
- If neither is available, the button is hidden or disabled in future UI.

Voice Watch does not inject into Roblox, read process memory, simulate clicks,
or bypass Roblox behavior.

## Project layout

```text
src/
  app_state.rs         Voice state machine
  countdown.rs         Local anchored countdown math
  ipc.rs               IPC bridge abstraction for the native host/tray link
  messages.rs          Shared sanitized protocol models
  monitor.rs           Polling decisions and backoff
  native_messaging.rs  Chrome/Edge native messaging framing
  overlay.rs           Restore notification adapter
  process.rs           Roblox process detection
  rejoin.rs            Last-server rejoin targets
  roblox_logs.rs       Best-effort Roblox log parsing
  settings.rs          Local settings load/save/validation
  tray.rs              Tray runtime and prototype menu

extension/
  manifest.json
  service_worker.js
  connect.html
  connect.css
  connect.js

scripts/
  register-native-host.ps1
  unregister-native-host.ps1
```

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) and
keep privacy and safety boundaries intact.

Run these before opening a pull request:

```powershell
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

Release packaging notes live in [docs/RELEASING.md](docs/RELEASING.md).

## License

MIT. See [LICENSE](LICENSE).

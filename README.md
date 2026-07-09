# Voice Watch

Voice Watch: A privacy-first lightweight Windows tray app that tracks Roblox
voice chat suspension timers and notifies you when VC is restored.

This project is not affiliated with Roblox.

## The problem Voice Watch solves

Roblox voice chat suspensions can leave you guessing. The Roblox client may keep
running after you leave a game, the suspension timer is not surfaced as a
desktop notification, and repeatedly checking the voice endpoint while VC is
already working is unnecessary.

Voice Watch exists to answer one practical question:

> "When can I use Roblox voice chat again?"

It runs quietly in the Windows tray, tracks the sanitized VC status returned by
Roblox, shows a local countdown when Roblox reports a temporary suspension, and
notifies you when VC is restored.

## What Voice Watch does

Voice Watch combines a local desktop app with a small browser connector:

1. The browser connector uses your existing Roblox browser login session to ask
   Roblox for voice settings.
2. The connector strips the response down to safe status fields and sends those
   fields to the local desktop app.
3. The desktop app renders the tray status, countdown, and restore notification.
4. Before each web check, the connector asks the desktop app whether polling
   makes sense.
5. The desktop app only allows normal polling while a real Roblox game window is
   visible.
6. If Windows reports that Roblox is actively using the microphone, Voice Watch
   pauses web checks because VC is already active.
7. If Roblox reports a temporary suspension with an end time, Voice Watch waits
   until that countdown expires before checking again.

This keeps checks focused on the moments where they are useful: while you are in
a Roblox game, not already using VC, and waiting for a suspension to clear. A
small HUD attaches near the top of the Roblox window while suspended so you can
see the remaining time without opening the tray menu. The HUD can be hidden for
the current suspension/restored phase without stopping the tray countdown or the
restore sound.

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

## Current status

Voice Watch currently includes:

- Rust desktop project structure with clean slices for state, settings, tray,
  native messaging, process detection, microphone activity detection, countdown
  logic, Roblox log parsing, and rejoin behavior.
- Native messaging host mode with Chromium browser framing.
- Manifest V3 browser extension that connects to the native host automatically.
- Real extension fetch to `https://voice.roblox.com/v1/settings` using
  `credentials: "include"` and no cookie permission.
- Sanitized voice status conversion, including `bannedUntilMs`.
- Local countdown anchoring that is resilient to system clock changes after the
  status fetch.
- Poll pause behavior while Roblox is using the microphone.
- Poll pause behavior while a known suspension countdown is still active.
- Visible Roblox game-window detection so the lingering background client does
  not keep checks running after you leave a game.
- Compact suspension HUD attached to the Roblox window, with a Rejoin button
  after VC is restored.
- A tray menu and native dialog notification fallback for restore alerts.
- Startup launch enabled by default from the installer.
- Development scripts for registering and unregistering the native messaging
  host.

The polished settings UI is planned follow-up work.

## Requirements

- Windows 10 or newer

Regular users should download the installer from GitHub Releases. Rust is only
needed for contributors.

Contributor requirements:

- Rust stable toolchain
- A Chromium-based browser for extension development
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
  "developerMode": false,
  "pausePollingWhileRobloxUsesMicrophone": true,
  "showOverlay": true,
  "playSoundOnRestore": true,
  "overlayPosition": "top-right",
  "launchOnStartup": true
}
```

`pollIntervalSeconds` is clamped to 10-300 seconds.
When `onlyPollWhenRobloxRunning` is enabled, Voice Watch waits for a visible
Roblox game window instead of trusting the lingering background client process.
When `pausePollingWhileRobloxUsesMicrophone` is enabled, Voice Watch pauses
Roblox web checks while Windows reports that Roblox is actively using the
microphone. Voice Watch does not read microphone audio; it only reads Windows'
local microphone-use metadata for the Roblox executable.
When Roblox returns a temporary suspension end time, the browser connector waits
for that local countdown to expire before asking Roblox again.

Set `developerMode` to `true` to show a tray-only **Test Suspend** button. It
starts a local two-minute test suspension for checking the countdown and HUD,
then disables while that suspension is active.

## Load the extension in a Chromium-based browser

For regular use, install Voice Watch, open the tray menu, and choose
**Connect Roblox**. Voice Watch opens the bundled setup page only when the
browser connector still needs to be loaded. When Voice Watch starts and the
browser connector is not connected yet, the setup page opens automatically.

For development:

1. Build the desktop app:

   ```powershell
   cargo build --release
   ```

2. Open `extension/setup.html`.
3. Choose your browser and follow the setup page.
4. In your browser, enable developer mode.
5. Choose **Load unpacked** and select the `extension/` folder.
6. Copy the generated extension ID.
7. Paste the extension ID into the setup page and choose
   **Register with Voice Watch**.

Manual fallback:

```powershell
cargo run -- --register-native-host "your-extension-id" --browser all
```

PowerShell fallback:

```powershell
.\scripts\register-native-host.ps1 -ExtensionId "your-extension-id" -Browser All
```

The extension popup is read-only. It shows desktop connection, VC status, and a
disconnect button only while connected.

Do not open `extension/connect.html` directly from the file system. It only works
inside the installed browser extension.

To remove the native messaging registration:

```powershell
.\scripts\unregister-native-host.ps1 -Browser All -RemoveManifest
```

## Rejoin last server

Rejoining is always user-clicked. Voice Watch never auto-rejoins.

The app tries to infer the last Roblox place and server ID from local Roblox
logs. This is best-effort:

- If `placeId` and an exact join target are available, the rejoin action opens
  Roblox's game-start link for that server.
- Public servers use `gameInstanceId`, also logged by Roblox as `GameId`.
- Private servers can use `accessCode` or `linkCode` when Roblox logs them.
- If only `placeId` is available, Voice Watch can identify the experience but
  cannot honestly claim an exact same-server rejoin, so the HUD disables the
  Rejoin button.

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
  native_messaging.rs  Chromium native messaging framing
  overlay.rs           Compact suspension HUD and restore notification adapter
  process.rs           Roblox window and microphone activity detection
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
  setup.html
  help.html

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

# Voice Watch

Voice Watch: A privacy-first lightweight Windows tray app that tracks Roblox
voice chat suspension timers and notifies you when VC is restored.

This project is not affiliated with Roblox.

<p>
  <a href="https://github.com/sponsors/Qxshio">
    <img src="https://img.shields.io/badge/Sponsor_on_GitHub-ea4aaa?style=for-the-badge&logo=githubsponsors&logoColor=white" alt="Sponsor Voice Watch on GitHub">
  </a>
  <a href="https://ko-fi.com/qxshio">
    <img src="https://img.shields.io/badge/Support_on_Ko--fi-ff5f5f?style=for-the-badge&logo=kofi&logoColor=white" alt="Support Voice Watch on Ko-fi">
  </a>
</p>

## Support the project

Voice Watch is built and maintained as an independent open source project. If it
helps you, donations make a real difference: they help me justify the time spent
fixing edge cases, testing across Windows and supported browsers, improving the
installer and release flow, and keeping the app free, privacy-first, and
uncluttered by ads or tracking.

- Sponsor on GitHub: <https://github.com/sponsors/Qxshio>
- Support on Ko-fi: <https://ko-fi.com/qxshio>

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

Release builds statically link the Microsoft C runtime, so fresh Windows
systems do not need a separate Visual C++ Redistributable install just to start
Voice Watch.

Voice Watch also checks GitHub Releases for newer public builds in the
background. When a newer installer is available, the tray icon turns amber and a
top-level `Update Available` button appears in the tray menu. Updates are
user-clicked: Voice Watch downloads the new installer, closes itself, lets the
installer refresh the app, then starts Voice Watch again.

Each connected browser reports its connector version during the local native
messaging handshake. If the desktop app is newer, the amber tray menu shows an
`Update extension` action. Clicking it asks every outdated connected browser to
perform a supported one-time store update check or reload the connector files
bundled by the desktop installer, then opens fallback instructions for older
connectors.
Equal versions need no action. A connector newer than the desktop app is never
downgraded; the normal desktop `Update Available` flow continues independently.

> [!IMPORTANT]
> Voice Watch 0.1.3 through 0.1.7 can be blocked by Windows when starting their
> update helper. Install 0.1.8 or newer manually once using the release installer.
> Your settings are preserved, and later updates can use the in-app updater.

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
7. If smart polling is enabled and Roblox has been mic-quiet for more than 20
   seconds after a clean "not suspended" result, Voice Watch skips web checks
   until local activity makes another check useful.
8. If Roblox reports a temporary suspension with an end time, Voice Watch waits
   until that countdown expires before checking again.

This keeps checks focused on the moments where they are useful: while you are in
a Roblox game, not already using VC, and waiting for a suspension to clear. A
small HUD attaches near the top of the Roblox window while suspended so you can
see the remaining time without opening the tray menu. The HUD can be hidden for
the current suspension/restored phase without stopping the tray countdown or the
restore sound, and the six-dot grip can be dragged to move the HUD without
resetting the countdown.

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

For the user-clicked Rejoin feature, the extension can separately send a
sanitized `placeId` plus `gameInstanceId`, `accessCode`, or `linkCode`. Cookies,
headers, and account tokens never cross the native messaging boundary.

See [docs/PRIVACY.md](docs/PRIVACY.md) for the full model.

## Current status

Voice Watch currently includes:

- Rust desktop project structure with clean slices for state, settings, tray,
  native messaging, process detection, microphone activity detection, countdown
  logic, Roblox log parsing, and rejoin behavior.
- Native messaging host mode for Chromium-based browsers and Firefox.
- Manifest V3 browser extension that connects to the native host automatically
  and uses browser-specific native-host manifests.
- Real extension fetch to `https://voice.roblox.com/v1/settings` using
  `credentials: "include"` and no cookie permission.
- Sanitized voice status conversion, including `bannedUntilMs`.
- Local countdown anchoring that is resilient to system clock changes after the
  status fetch.
- Poll pause behavior while Roblox is using the microphone.
- User-clicked auto-update flow from GitHub Releases.
- Per-browser connector version checks with a user-clicked extension update
  action and no automatic downgrade path.
- Smart polling pause behavior when Roblox has been mic-quiet for more than 20
  seconds without a suspension.
- Poll pause behavior while a known suspension countdown is still active.
- Current-server presence checks only while a game is visible, throttled to once
  per minute with a short-lived local user-ID cache.
- Visible Roblox game-window detection so the lingering background client does
  not keep checks running after you leave a game.
- Compact suspension HUD attached to the Roblox window, with a Rejoin button
  after VC is restored.
- A tray menu, compact native HUD, and local restore sound.
- Native settings window for polling, HUD, startup, sound, and developer-mode
  preferences.
- Startup launch enabled by default and governed by the saved app setting across
  upgrades.
- Development scripts for registering and unregistering the native messaging
  host.

## Requirements

- Windows 10 or newer

Regular users should download the installer from GitHub Releases. Rust is only
needed for contributors.

Contributor requirements:

- Rust stable toolchain
- A Chromium-based browser or Firefox for extension development
- Inno Setup 6 for building the Windows installer

Install Rust from <https://rustup.rs/> if `cargo` is not available.

## Build the desktop app

```powershell
cargo build --release
```

Useful development commands:

```powershell
cargo run -- --native-host
cargo run -- --print-config-path
```

The settings file is created under `%APPDATA%\Voice Watch\settings.json`.
If that file is invalid, Voice Watch preserves it as
`settings.invalid-<timestamp>-<pid>.json` and starts with validated defaults.

## Default settings

```json
{
  "pollIntervalSeconds": 10,
  "onlyPollWhenRobloxRunning": true,
  "developerMode": false,
  "pausePollingWhileRobloxUsesMicrophone": true,
  "smartPolling": true,
  "showOverlay": true,
  "playSoundOnRestore": true,
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
When `smartPolling` is enabled, Voice Watch pauses Roblox web checks after the
mic has been quiet for more than 20 seconds and the last successful Roblox
status said you were not suspended.
When Roblox returns a temporary suspension end time, the browser connector waits
for that local countdown to expire before asking Roblox again.

Set `developerMode` to `true` to show a tray-only **Test Suspend** button. It
starts a local two-minute test suspension for checking the countdown and HUD,
then disables while that suspension is active.

## Load the browser extension

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
6. The extension opens a **Finish Voice Watch setup** tab.
7. Choose **Finish setup** and allow Windows to open Voice Watch.

The source `extension/` folder is the Chromium development build. For Firefox,
use the generated `voice-watch-connector-firefox-<version>.zip`; the packaging
script writes Firefox-specific background and add-on ID metadata.

If the finish tab does not appear, open the Voice Watch extension icon in the
browser toolbar and choose **Finish setup** there. The extension supplies its
own browser ID automatically, so normal setup does not require copying or
pasting an extension ID.

Manual fallback:

```powershell
cargo run -- --register-native-host "your-extension-id" --browser all
```

PowerShell fallback:

```powershell
.\scripts\register-native-host.ps1 -ExtensionId "your-extension-id" -Browser All
```

The extension popup is status-focused: it shows desktop connection and VC
status, offers **Disconnect** while connected, and offers **Reconnect desktop**
after an intentional disconnect. Reconnecting does not require removing or
reinstalling the extension.

Do not open `extension/connect.html` directly from the file system. It only works
inside the installed browser extension.

To remove the native messaging registration:

```powershell
.\scripts\unregister-native-host.ps1 -Browser All -RemoveManifest
```

## Extension packaging

Regular users do not need to build the browser extension. The installer bundles
the readable, unpacked `extension/` folder so privacy-minded users can inspect
the files before loading them manually.

Store ZIPs are maintainer upload artifacts for Chrome Web Store, Microsoft Edge
Add-ons, and Firefox AMO. Build them with:

```powershell
.\scripts\package-extensions.ps1
```

The script regenerates browser icon PNGs from `assets/logo.svg`, updates
`extension/icons/`, and writes store upload packages to `dist/`.

For Firefox AMO review, the script also writes
`dist/voice-watch-connector-firefox-source-<version>.zip`. Upload that archive
when Mozilla asks for source code, and upload
`dist/voice-watch-connector-firefox-<version>.zip` as the add-on package.

## Rejoin last server

Rejoining is always user-clicked. Voice Watch never auto-rejoins.

The extension detects the signed-in user's current server while a Roblox game
is visible. Local Roblox logs are used only as a best-effort fallback:

- If `placeId` and an exact join target are available, the desktop sends a
  typed rejoin command to the connected extension. That browser opens a marked
  Roblox game page and calls Roblox's page launcher for the exact server.
- Public servers use `gameInstanceId`, also logged by Roblox as `GameId`.
- Private servers can use `accessCode` or `linkCode` when Roblox logs them.
- The local `roblox://` deep link remains a fallback if the desktop cannot queue
  the browser command.
- If only `placeId` is available, Voice Watch can identify the experience but
  cannot honestly claim an exact same-server rejoin, so the HUD disables the
  Rejoin button.

Voice Watch does not inject into the Roblox client, read process memory,
simulate clicks, or bypass Roblox behavior. The browser helper only observes
Roblox web launch metadata and maintains a fallback page launcher.

## Project layout

```text
src/
  app_state.rs         Voice state machine
  countdown.rs         Local anchored countdown math
  ipc.rs               Persisted event and desktop-command bridge
  messages.rs          Shared sanitized protocol models
  native_messaging.rs  Browser native messaging framing and readiness logic
  overlay.rs           Compact suspension/restored HUD and local sound
  process.rs           Roblox window and microphone activity detection
  rejoin.rs            Last-server rejoin targets
  roblox_logs.rs       Best-effort Roblox log parsing
  settings.rs          Local settings load/save/validation
  tray.rs              Tray runtime, menu, state wiring, and update prompts

extension/
  icons/
  manifest.json
  service_worker.js
  connect.html
  connect.css
  connect.js
  rejoin.js
  rejoin_page.js
  setup.html
  help.html

assets/
  logo.svg

scripts/
  package-extensions.ps1
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

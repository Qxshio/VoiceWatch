# Native Messaging

Voice Watch uses browser native messaging to connect Chromium-based browsers
and Firefox to the local desktop app.

## Host name

```text
com.voice_watch.native
```

## Registration

During normal setup, the extension opens its own finish page after it is loaded.
That page sends its browser-generated extension ID to the desktop app through a
`voice-watch://register-native-host` link, so users do not need to copy the ID
by hand.

During development, the manual registration fallback is:

```powershell
cargo build --release
.\scripts\register-native-host.ps1 -ExtensionId "your-extension-id" -Browser All
```

Chromium registration writes:

```text
%LOCALAPPDATA%\VoiceWatch\native-messaging\com.voice_watch.native.json
```

Firefox registration writes a separate manifest because Firefox uses
`allowed_extensions` rather than Chromium's `allowed_origins`:

```text
%LOCALAPPDATA%\VoiceWatch\native-messaging\com.voice_watch.native-firefox.json
```

The manifests are registered under current-user browser keys. Firefox uses
`HKCU\Software\Mozilla\NativeMessagingHosts\com.voice_watch.native`; Chromium
forks use their corresponding `NativeMessagingHosts` keys.

Opera is also registered through the Chrome-compatible native messaging key.
Opera's Windows native messaging documentation points to the Chrome registry
location, so Voice Watch writes both that key and the Opera-specific key for
compatibility. Opera GX also gets an Opera GX-specific fallback registry key.

Chromium starts the executable with the calling `chrome-extension://` origin.
Firefox passes the manifest path and the fixed Voice Watch add-on ID. Voice
Watch recognizes both invocation shapes before processing normal desktop
arguments. The explicit `--native-host` flag remains available for manual
testing.

## Protocol

Extension to host:

```json
{
  "type": "hello",
  "extensionVersion": "0.1.11",
  "protocolVersion": 1
}
```

Host to extension:

```json
{
  "type": "hello_ack",
  "appVersion": "0.1.11",
  "protocolVersion": 1,
  "pollIntervalSeconds": 10
}
```

Before each Roblox web request, the extension asks the host whether polling is
useful:

```json
{
  "type": "poll_readiness_request",
  "requestId": "uuid"
}
```

Host readiness response:

```json
{
  "type": "poll_readiness",
  "requestId": "uuid",
  "pollIntervalSeconds": 10,
  "shouldPoll": false,
  "robloxRunning": true,
  "robloxPlaying": true,
  "microphoneActive": true,
  "reason": "microphone_active",
  "message": "Roblox is using the microphone, so voice chat is active."
}
```

If `shouldPoll` is false, the extension skips the Roblox web request for that
interval. The host returns false when no visible Roblox game window exists, or
when Windows reports Roblox is actively using the microphone. With smart
polling enabled, it also returns false after Roblox has been mic-quiet for more
than 20 seconds and the last successful Roblox status said the user was not
suspended.

Manual app request to extension:

```json
{
  "type": "check_voice_status",
  "requestId": "uuid"
}
```

Extension response:

```json
{
  "type": "voice_status",
  "requestId": "uuid",
  "checkedAt": 1783548000000,
  "ok": true,
  "data": {
    "isVoiceEnabled": false,
    "isUserOptIn": true,
    "isUserEligible": false,
    "isBanned": true,
    "banReason": 7,
    "bannedUntilMs": 1783549092985,
    "denialReason": 6
  }
}
```

Errors use the same `voice_status` envelope with `ok: false`.

User-clicked rejoin command from the desktop:

```json
{
  "type": "rejoin",
  "server": {
    "placeId": 123,
    "gameInstanceId": "1bb8dd1d-ad4c-43d2-a9c6-63feee836e43",
    "accessCode": null,
    "linkCode": null,
    "detectedAtMs": 1783548000000
  }
}
```

The extension opens the marked Roblox rejoin page in the same browser that owns
the native connection. Rejoin is never sent without a user click.

When the handshake reports a connector version older than the desktop app, the
tray exposes a user-clicked extension update command targeted to that browser's
native-host process:

```json
{
  "type": "update_extension",
  "desktopVersion": "0.1.11"
}
```

On Chromium browsers, the connector performs one browser-managed update check.
If no packaged update is available, it reloads itself so an unpacked connector
reads the files already updated by the Voice Watch installer. Firefox does not
provide Chromium's manual update-check API, so its connector reloads and relies
on Firefox's normal add-on updater. `runtime.onUpdateAvailable` reloads a store
connector once a new package is ready. Voice Watch does not run an extension
update timer, and a connector newer than the desktop app is never downgraded.

Intentional extension disconnect:

```json
{
  "type": "disconnect"
}
```

## Frame format

Browser native messaging frames are:

1. Four-byte little-endian unsigned payload length.
2. UTF-8 JSON payload.

`src/native_messaging.rs` caps frames at 1 MiB.

## Tray bridge

Browser native-host mode and the tray app are separate processes. `src/ipc.rs`
connects them with a bounded JSONL event log plus atomic shared and per-host
desktop-command files under `%APPDATA%\Voice Watch`. Each live-host marker keeps
its process ID, connection time, and sanitized connector version. The last
sanitized voice status and last server are persisted separately so tray restarts
retain countdowns and do not mistake one browser disconnect for all browsers
disconnecting.

# Native Messaging

Voice Watch uses Chromium native messaging to connect the browser extension to
the local desktop app.

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

The script writes a native messaging manifest under:

```text
%LOCALAPPDATA%\VoiceWatch\native-messaging\com.voice_watch.native.json
```

It then registers that manifest under the current user's supported Chromium
browser registry keys.

Opera is also registered through the Chrome-compatible native messaging key.
Opera's Windows native messaging documentation points to the Chrome registry
location, so Voice Watch writes both that key and the Opera-specific key for
compatibility. Opera GX also gets an Opera GX-specific fallback registry key.

The browser starts the executable listed in the manifest and passes the calling
extension origin as the first argument, for example
`chrome-extension://<extension-id>`. Voice Watch treats that argument as native
host mode. The explicit `--native-host` flag is still available for manual
testing.

## Protocol

Extension to host:

```json
{
  "type": "hello",
  "extensionVersion": "0.1.6",
  "protocolVersion": 1
}
```

Host to extension:

```json
{
  "type": "hello_ack",
  "appVersion": "0.1.6",
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

Legacy app request to extension:

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

Intentional extension disconnect:

```json
{
  "type": "disconnect"
}
```

## Frame format

Chromium native messaging frames are:

1. Four-byte little-endian unsigned payload length.
2. UTF-8 JSON payload.

`src/native_messaging.rs` caps frames at 1 MiB.

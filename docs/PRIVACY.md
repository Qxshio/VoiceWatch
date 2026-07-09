# Privacy

Voice Watch deliberately avoids browser cookie access.

Voice Watch does not read your browser cookie database.  
Voice Watch does not extract `.ROBLOSECURITY`.  
Voice Watch does not upload Roblox cookies or tokens.  
The browser extension uses your existing browser login session to request VC
status from Roblox and sends only sanitized status data to the local desktop
app.

## Why a browser extension

Roblox voice status requires an authenticated browser session. Reading cookies
from browser profile files would be invasive and unsafe. Instead, the extension
lets the browser perform the request using its normal session handling:

```js
fetch("https://voice.roblox.com/v1/settings", {
  credentials: "include"
});
```

The extension does not request the `cookies` permission.

The extension also has access to Roblox game pages for the Rejoin helper. That
helper only runs when a user-clicked Voice Watch link includes
`voiceWatchRejoin=1`, and it uses Roblox's page launcher instead of sending
cookies or tokens to the desktop app.

## What is sent to the desktop app

Only this sanitized shape is sent:

```json
{
  "type": "voice_status",
  "requestId": "some-id",
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

The raw Roblox response is not forwarded. Cookies, headers, and tokens are not
forwarded.

## Local files

The desktop app stores settings under:

```text
%APPDATA%\Voice Watch\settings.json
```

Best-effort last-server detection reads local Roblox log files under:

```text
%LOCALAPPDATA%\Roblox\logs
```

This is used only to detect best-effort rejoin metadata such as `placeId`,
`gameInstanceId`/`GameId`, `accessCode`, or `linkCode` when available. The app
does not read Roblox memory, inject into the Roblox client, or manipulate the
Roblox process.

## Microphone activity

Voice Watch does not record, inspect, transmit, or analyze microphone audio.

On Windows, the desktop app reads local microphone-use metadata from the current
user's privacy registry entries for the running Roblox executable. This only
answers whether Windows currently considers Roblox to be using the microphone.
Voice Watch uses that signal to pause Roblox web checks while VC is already
active.

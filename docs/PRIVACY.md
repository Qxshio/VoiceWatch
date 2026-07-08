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

This is used only to detect `placeId` and `gameInstanceId` when available. The
app does not read Roblox memory, inject into Roblox, or manipulate the Roblox
process.

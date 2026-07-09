# Voice Watch Connector Extension

This Manifest V3 extension is the safe authentication bridge for Voice Watch.
It uses the browser's existing Roblox login session to call
`https://voice.roblox.com/v1/settings`, then sends only sanitized voice status
fields to the local desktop app through Chromium native messaging.

It does not request the `cookies` permission, does not read browser cookie
storage, and does not send `.ROBLOSECURITY` or any other token to the desktop
app.

## Rejoin helper

The extension detects the logged-in user's current Roblox presence and sends
only sanitized `placeId`/`gameInstanceId` metadata to the desktop app. The
desktop Rejoin button then opens Roblox directly with a user-clicked
`roblox://` deep link.

The game-page script also watches Roblox web launch calls and keeps a fallback
launcher for marked Voice Watch rejoin URLs. It does not read browser cookies or
send Roblox tokens to the desktop app.

## How polling works

The extension does not blindly call Roblox forever. Before each voice-status
request, it asks the desktop app whether a check is useful.

The desktop app tells it to skip the request when:

- no visible Roblox game window is open,
- Roblox is already using the microphone.

The extension also sleeps locally while a known suspension countdown from the
last Roblox response has not expired yet.

When a check is allowed, the extension calls
`https://voice.roblox.com/v1/settings` with `credentials: "include"` and sends
only sanitized status fields back to the local app.

## Development loading

1. Open `setup.html`.
2. Choose your browser and follow the page.
3. Enable developer mode in that browser.
4. Choose **Load unpacked**.
5. Select this `extension/` folder.
6. Copy the generated extension ID.
7. Register the native host with that ID from the setup page.

Manual fallback:

```powershell
.\scripts\register-native-host.ps1 -ExtensionId "your-extension-id" -Browser All
```

Do not open `connect.html` directly from the file system. It is the extension
popup and only has access to browser extension APIs after a supported browser
loads this folder as an extension.

# Voice Watch Connector Extension

This Manifest V3 extension is the safe authentication bridge for Voice Watch.
It uses the browser's existing Roblox login session to call
`https://voice.roblox.com/v1/settings`, then sends only sanitized voice status
fields to the local desktop app through Chrome/Edge native messaging.

It does not request the `cookies` permission, does not read browser cookie
storage, and does not send `.ROBLOSECURITY` or any other token to the desktop
app.

## Development loading

1. Open `chrome://extensions` or `edge://extensions`.
2. Enable developer mode.
3. Choose **Load unpacked**.
4. Select this `extension/` folder.
5. Copy the generated extension ID.
6. Register the native host with that ID:

```powershell
.\scripts\register-native-host.ps1 -ExtensionId "your-extension-id" -Browser Both
```

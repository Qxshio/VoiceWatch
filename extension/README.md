# Voice Watch Connector Extension

This Manifest V3 extension is the safe authentication bridge for Voice Watch.
It uses the browser's existing Roblox login session to call
`https://voice.roblox.com/v1/settings`, then sends only sanitized voice status
fields to the local desktop app through Chromium native messaging.

It does not request the `cookies` permission, does not read browser cookie
storage, and does not send `.ROBLOSECURITY` or any other token to the desktop
app.

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

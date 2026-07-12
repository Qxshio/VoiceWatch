# Voice Watch Connector Extension

This Manifest V3 extension is the safe authentication bridge for Voice Watch.
It uses the browser's existing Roblox login session to call
`https://voice.roblox.com/v1/settings`, then sends only sanitized voice status
fields to the local desktop app through browser native messaging. Release
packaging produces Chromium and Firefox-specific manifests from the same
readable source.

It does not request the `cookies` permission, does not read browser cookie
storage, and does not send `.ROBLOSECURITY` or any other token to the desktop
app.

## Rejoin helper

While the desktop reports a visible Roblox game, the extension refreshes the
logged-in user's current Roblox presence at most once per minute and sends only
sanitized `placeId`/`gameInstanceId` metadata to the desktop app. The desktop
Rejoin button sends a typed command back through the native host, so the same
browser opens Roblox's exact-server page launcher. A local `roblox://` link is
kept only as a fallback.

The game-page script also watches Roblox web launch calls and keeps a fallback
launcher for marked Voice Watch rejoin URLs. It does not read browser cookies or
send Roblox tokens to the desktop app.

## How polling works

The extension does not blindly call Roblox forever. Before each voice-status
request, it asks the desktop app whether a check is useful.

The desktop app tells it to skip the request when:

- no visible Roblox game window is open,
- Roblox is already using the microphone,
- smart polling sees more than 20 seconds of mic silence after a clean
  not-suspended result.

The extension also sleeps locally while a known suspension countdown from the
last Roblox response has not expired yet, and respects Roblox's `Retry-After`
delay after a rate-limited response.

When a check is allowed, the extension calls
`https://voice.roblox.com/v1/settings` with `credentials: "include"` and sends
only sanitized status fields back to the local app.

## Development loading

1. Open `setup.html`.
2. Choose your browser and follow the page.
3. Enable developer mode in that browser.
4. Choose **Load unpacked**.
5. Select this `extension/` folder.
6. Use the **Finish Voice Watch setup** tab that opens from the extension.

If the finish tab does not open, click the Voice Watch extension icon in the
browser toolbar and choose **Finish setup**. The extension provides its own ID
to the desktop app automatically.

After an intentional disconnect, open the same popup and choose **Reconnect
desktop**. The existing native-host registration is reused, so the extension
does not need to be removed or installed again.

## Version compatibility

The connector includes its manifest version in the local desktop handshake. If
the desktop app is newer, Voice Watch shows `Update extension` in the tray. That
user click requests one browser-managed update check where the API is supported;
unpacked installations reload from the connector files updated by the desktop
installer. Firefox relies on its normal add-on updater because it does not expose
Chromium's manual update-check API. Store updates reload when the browser reports
that the new package is ready. The connector does not check for updates on a
timer, and a newer connector is never asked to downgrade.

Manual fallback:

```powershell
.\scripts\register-native-host.ps1 -ExtensionId "your-extension-id" -Browser All
```

Do not open `connect.html` directly from the file system. It is the extension
popup and only has access to browser extension APIs after a supported browser
loads this folder as an extension.

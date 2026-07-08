# Safety Boundaries

Voice Watch must not become a credential tool or a Roblox automation bypass.

Do not implement:

- Browser cookie scanning.
- Browser cookie decryption.
- `.ROBLOSECURITY` extraction.
- Sending cookies to the native app.
- Sending cookies to any server.
- Automatic rejoin without user click.
- Roblox process injection.
- Roblox memory reading.
- Click simulation.
- Anti-cheat bypassing.
- Anything that resembles credential theft.

Allowed behavior:

- Browser extension requests VC status using the browser's normal logged-in
  session.
- Extension sends sanitized voice status fields to the local app.
- Desktop app detects whether `RobloxPlayerBeta.exe` is running.
- Desktop app reads local Roblox logs for `placeId` and `gameInstanceId` only.
- Desktop app opens a Roblox link after the user clicks a rejoin button.

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
- Microphone audio recording or analysis.
- Click simulation.
- Anti-cheat bypassing.
- Anything that resembles credential theft.

Allowed behavior:

- Browser extension requests VC status using the browser's normal logged-in
  session.
- Extension sends sanitized voice status fields to the local app.
- Desktop app detects whether `RobloxPlayerBeta.exe` has a visible game window.
- Desktop app reads Windows microphone-use metadata for the current Roblox
  executable path, only to pause polling while VC is already active.
- Desktop app reads local Roblox logs for best-effort rejoin metadata such as
  `placeId`, `gameInstanceId`/`GameId`, `accessCode`, or `linkCode`.
- Browser extension reads Roblox presence for the signed-in browser user and
  sends only sanitized `placeId`/`gameInstanceId` metadata to the local app.
- Desktop app opens a Roblox app deep link after the user clicks a rejoin
  button.
- Extension uses Roblox's page launcher only on the user-opened rejoin page and
  only with the explicit metadata in that URL.

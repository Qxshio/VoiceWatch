# Roadmap

## Prototype

- [x] Create Rust desktop project slices.
- [x] Add Manifest V3 extension.
- [x] Implement safe voice-status fetch in the extension.
- [x] Add native messaging frame support.
- [x] Add countdown state and local countdown math.
- [x] Add basic tray runtime.
- [x] Add basic restore notification fallback.
- [x] Add best-effort Roblox log parser.
- [x] Add user-clicked rejoin target helper.
- [x] Add compact Roblox-window suspension HUD.
- [x] Add launch-on-startup integration.
- [x] Add docs and setup scripts.
- [x] Add Windows installer packaging and release artifact workflow.

## Next

- [ ] Implement named-pipe IPC between native host mode and running tray app.
- [ ] Let the tray app request status checks from the connected extension.
- [ ] Add a settings/status native window.
- [ ] Add graceful rate-limit and repeated-failure backoff in the runtime loop.
- [ ] Improve last-server parsing with real-world Roblox log samples.
- [ ] Add release packaging and code signing notes.
- [ ] Add installer flow for native messaging registration.

## Later

- [ ] Optional sound notification.
- [ ] Better multi-monitor overlay placement.
- [ ] Localization.
- [ ] Public extension store packaging.

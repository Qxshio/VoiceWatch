# Contributing

Thanks for helping make Voice Watch better. This project aims to be useful,
small, and easy to audit.

## Development principles

- Keep privacy boundaries explicit.
- Prefer small modules with one clear responsibility.
- Keep UI work native and lightweight.
- Add tests for state transitions, protocol parsing, countdown math, and log
  parsing.
- Document user-visible behavior changes.

## Safety boundaries

Do not add code that:

- Reads browser cookie databases.
- Extracts `.ROBLOSECURITY`.
- Sends cookies, tokens, headers, or raw auth material to the desktop app.
- Uploads Roblox credentials or tokens anywhere.
- Injects into Roblox.
- Reads Roblox process memory.
- Simulates clicks or bypasses Roblox behavior.
- Auto-rejoins without an explicit user click.

## Pull request checklist

- The change is narrowly scoped.
- `cargo fmt --all` passes.
- `cargo clippy --all-targets -- -D warnings` passes.
- `cargo test` passes.
- Extension changes still avoid the `cookies` permission.
- README or docs are updated when behavior changes.

## Local setup

```powershell
cargo build
cargo test
cargo run -- --simulate-suspension 10
```

For extension development, load `extension/` as an unpacked extension and
register the native host with `scripts/register-native-host.ps1`.

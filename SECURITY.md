# Security Policy

Voice Watch handles voice status metadata only. It must never handle Roblox
session cookies or tokens.

## Reporting a vulnerability

Please open a private security advisory if this repository is hosted on GitHub,
or email the maintainers listed by the project owner.

Include:

- A short description of the issue.
- Reproduction steps.
- Impact.
- Suggested fix, if known.

## Explicit non-goals

The project will not implement:

- Browser cookie scanning.
- Browser cookie decryption.
- `.ROBLOSECURITY` extraction.
- Credential exporting.
- Roblox process injection.
- Roblox memory reading.
- Click simulation.
- Anti-cheat bypasses.

Reports or pull requests that add these behaviors will be rejected.

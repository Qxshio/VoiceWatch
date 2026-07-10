# Voice Watch Connector Firefox Source Build

This source package is for Mozilla AMO review of the Voice Watch Connector
Firefox extension.

## What is generated

The submitted extension is not minified, bundled, transpiled, concatenated, or
obfuscated. The JavaScript, HTML, and CSS files are copied from `extension/` as
plain source files.

The packaging script performs only these build steps:

- regenerates PNG extension icons from `assets/logo.svg`;
- copies the readable extension files from `extension/`;
- creates Firefox-specific manifest metadata, including the Gecko extension ID
  and the required no-data-collection declaration;
- writes the final Firefox upload ZIP with web-style `/` archive paths.

## Build environment

Required:

- Windows 10, Windows 11, or Windows Server 2022
- Windows PowerShell 5.1 or newer

No Node.js, npm, Rust, Cargo, webpack, template engine, minifier, transpiler, or
third-party build package is required to build the Firefox extension package.

Windows PowerShell 5.1 is installed by default on supported Windows versions. If
it is missing, install or repair Windows PowerShell through Microsoft's Windows
Management Framework for your Windows version.

Optional validation only:

- Node.js 20 or newer, if you want to run Mozilla's `addons-linter` locally

Node.js can be installed from <https://nodejs.org/> or with:

```powershell
winget install OpenJS.NodeJS.LTS
```

## Build steps

1. Extract this source ZIP.
2. Open Windows PowerShell in the extracted folder.
3. Run:

   ```powershell
   powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\package-extensions.ps1
   ```

4. The Firefox add-on upload ZIP is written to:

   ```text
   dist\voice-watch-connector-firefox-<version>.zip
   ```

   The `<version>` value is read from `Cargo.toml`.

5. Optional Mozilla linter check:

   ```powershell
   npx.cmd --yes addons-linter@latest .\dist\voice-watch-connector-firefox-<version>.zip
   ```

The script also writes Chrome and Edge upload ZIPs, plus a matching Firefox
source-review ZIP. Those extra ZIPs are not required to reproduce the Firefox
extension submitted to AMO.

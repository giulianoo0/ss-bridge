# ss-bridge

Small native helper for [ss](https://ss.giuli.dev): it embeds a torrent engine
(`librqbit`), downloads on the host's own machine, and serves the bytes to the
site over `http://127.0.0.1:32227`.

## Run locally

    cargo run --release

Then open ss — the header shows **ss-bridge** in green when it is connected.

macOS needs the Metal toolchain once: `xcodebuild -downloadComponent MetalToolchain`.

## HTTP API

- `GET  /health` — `{ name, version }`
- `POST /add` `{ magnet }` — `{ id, name, files: [{ index, name, path, size }] }`
- `POST /select` `{ id, fileIndex }`
- `GET  /stats/{id}` — `{ peers, downloadSpeed, uploadSpeed, downloaded, progress }` plus
  peer counters for diagnosing a slow swarm: `queued`, `connecting`, `seen`,
  `dead`, `notNeeded`
- `GET  /stream/{id}/{index}` (Range) — 206, blocks until the range has downloaded
- `POST /close` `{ id }`

## Releases (GitHub Actions)

Push a tag `vX.Y.Z` (or run the workflow manually) to build and publish
Windows, Linux and macOS artifacts. macOS is a universal `.app` inside a `.dmg`,
signed and notarized when these repo secrets are set:

- `MACOS_SIGNING_IDENTITY` — e.g. `Developer ID Application: Flashz (TEAMID)`
- `MACOS_CERTIFICATE` — base64 of the Developer ID `.p12`
- `MACOS_CERTIFICATE_PWD` — the `.p12` password
- `APPLE_ID`, `APPLE_APP_PASSWORD`, `APPLE_TEAM_ID` — for notarization (optional)

Without the secrets the macOS build still runs, unsigned.

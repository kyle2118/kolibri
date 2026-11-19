# Kolibri

A fast, minimal desktop HTTP client. No accounts, no login, no cloud, no AI,
no workspaces, and **no variables**: every URL, header, param and body is sent
exactly as you typed it (`{{token}}` goes out as the literal text `{{token}}`).

## Features

- **Sidebar** with folders and requests
  - Drag folders to reorder them; drag requests to reorder or move them to another folder
  - Double-click (or right-click → Rename, or F2) to rename a folder or request
  - Hover a folder and click **+** to add a request; right-click for Duplicate / Delete
- **Request editor**: method, URL, Send (Enter in the URL bar or Ctrl/Cmd+Enter),
  Params / Headers / Body tabs. Params and the URL query stay in sync.
- **cURL import**: paste a `curl ...` command into the URL bar to fill in method, URL,
  params, headers and body (values kept exactly as written; nothing is expanded).
- **Beautify** formats a JSON body (Ctrl/Cmd+B). Key order and numbers are preserved exactly.
- **Response**: status, time, size, highlighted body (JSON pretty-printed), headers, Copy.
- **Load test**: send the current request N times, or for N seconds, with C parallel
  workers. Shows requests/sec, min/avg/p50/p90/p95/p99/max latency, status code counts and errors.

## Data

Everything is saved automatically to one JSON file:

| OS      | Path |
|---------|------|
| macOS   | `~/Library/Application Support/kolibri/collections.json` |
| Linux   | `~/.config/kolibri/collections.json` |
| Windows | `%APPDATA%\kolibri\collections.json` |

Set `KOLIBRI_DATA=/path/to/file.json` to use another file.

## Build

Requires Rust (stable).

```sh
cargo run --release          # run
cargo build --release        # binary at target/release/kolibri
cargo test                   # unit tests
```

On macOS, `scripts/bundle-macos.sh` builds `target/release/Kolibri.app` with the app
icon, which you can drag to `/Applications`.

Built with [egui/eframe](https://github.com/emilk/egui) (OpenGL via glow) and
[ureq](https://github.com/algesten/ureq) (blocking HTTP on background threads,
rustls with the OS certificate store).

## Code layout

- `src/main.rs` window setup
- `src/theme.rs` design system: dark and light palettes, typography, spacing, sizes,
  radii, and the global egui style built from them
- `src/kit.rs` reusable components: vector icons, buttons, segmented tabs, badges,
  checkboxes, menu items, cards, callouts, empty states
- `src/app.rs` screens: sidebar, drag and drop, request editor, response, load test, dialogs
- `src/model.rs` data model, JSON persistence, URL/params syncing
- `src/curl.rs` parsing pasted curl commands
- `src/http.rs` building and sending requests, JSON beautify
- `src/loadtest.rs` load test workers and stats
- `src/highlight.rs` small JSON syntax highlighter
- `src/qa.rs` visual QA tour (development only, see below)
- `assets/` app icon: `icon.png` (1024 master), `icon-512.png` (embedded window/Dock icon),
  `icon-64.png` (sidebar logo), `kolibri.icns` (macOS bundle), `icon-source.png` (original artwork)
- `assets/fonts/` Inter and JetBrains Mono (Latin, Latin Extended, Cyrillic), both under the
  SIL Open Font License (`Inter-OFL.txt`, `JetBrainsMono-OFL.txt`)
- `scripts/bundle-macos.sh` builds the macOS `.app` bundle

## Design

Colors, fonts and sizes live only in `src/theme.rs`; screens use semantic names
(`bg_surface`, `text_muted`, `accent`, `space::MD`, `size::CONTROL`, ...) and the
components in `src/kit.rs`. The app follows the system light/dark setting.

## Visual QA

A scripted screenshot tour checks every major screen in both themes and at several
window sizes. It runs against a local mock API with demo data and never touches your
saved requests:

```sh
KOLIBRI_DATA=/tmp/kolibri-qa.json KOLIBRI_SHOTS=/tmp/kolibri-shots cargo run --features qa
```

PNG screenshots are written to the `KOLIBRI_SHOTS` folder, then the app quits.

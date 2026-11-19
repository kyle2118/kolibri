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

Built with [egui/eframe](https://github.com/emilk/egui) (OpenGL via glow) and
[ureq](https://github.com/algesten/ureq) (blocking HTTP on background threads,
rustls with the OS certificate store).

## Code layout

- `src/main.rs` window setup
- `src/app.rs` all UI: sidebar, drag and drop, editor, response, load test view
- `src/model.rs` data model, JSON persistence, URL/params syncing
- `src/curl.rs` parsing pasted curl commands
- `src/http.rs` building and sending requests, JSON beautify
- `src/loadtest.rs` load test workers and stats
- `src/highlight.rs` small JSON syntax highlighter

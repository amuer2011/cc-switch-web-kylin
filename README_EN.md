# CC Switch — Native Desktop Application

English | [中文](README.md)

> Original Tauri 2 desktop documentation: [README_tauri.md](README_tauri.md) · [README_TAURI_ZH.md](README_TAURI_ZH.md) (中文) · [README_TAURI_JA.md](README_TAURI_JA.md) (日本語)

**CC Switch** is a comprehensive management tool for AI CLI tool configurations, supporting Claude Code, Codex, Gemini CLI, OpenCode, OpenClaw, and Hermes.

This repository is an adaptation of [CC Switch](https://github.com/farion1231/cc-switch) that refactors the Tauri 2 desktop application into a **native GTK+WebKit desktop application** while preserving all core business logic. It runs on **Ubuntu 20.04** and other systems that lack Tauri 2 dependencies.

> Original Tauri 2 desktop documentation: [README_tauri.md](README_tauri.md)

---

## Supported Environments

| Environment | Status |
|-------------|--------|
| **Ubuntu 20.04 (Focal)** | ✅ Primary target |
| Ubuntu 22.04+ / Debian | ✅ Works without Tauri desktop features |
| Systems with WebKit2GTK 4.0 | ✅ Full native window support |

### Why not Tauri 2?

Ubuntu 20.04 lacks required Tauri 2 dependencies:
- `libwebkit2gtk-4.1-dev` (20.04 only has 4.0)
- `glib-2.0 >= 2.70` (20.04 has 2.64)
- `libsoup-3.0-dev`

This fork uses conditional compilation (`desktop` feature) to make all Tauri dependencies optional. The frontend is rendered via an embedded WebKit view (WebKit2GTK 4.0), no browser needed.

---

## Features

CC Switch retains **all core business logic** (60+ API endpoints):

| Category | Features |
|----------|----------|
| **Provider Management** | CRUD, switching, reordering, 50+ official presets, live config sync, universal providers |
| **MCP Server Management** | Unified MCP server management across 5 AI tools (Claude/Codex/Gemini/OpenCode/Hermes) |
| **Prompts/Skills** | Markdown editor, GitHub repo installation, cross-app sync, SSOT storage, auto-import from app directories |
| **Proxy** | Local HTTP proxy, auto failover, circuit breaker, live takeover/restore, hot-switching |
| **Session Manager** | Browse/search/restore conversation history for all supported tools |
| **Usage/Cost Tracking** | Request logs, token statistics, cost summaries, provider/model comparison charts |
| **WebDAV Sync** | Cloud sync for database and Skills |
| **Terminal Launcher** | One-click terminal with provider environment variables injected |
| **Config Snippets** | Share configuration templates across providers |
| **Custom Endpoints** | Add/manage API endpoints per provider |
| **Claude Plugin** | Auto-configure Claude Code plugin settings |
| **Launch on Startup** | XDG autostart toggle in settings |

---

## Unsupported Features

These Tauri-dependent features are not available:

| Feature | Reason | Alternative |
|---------|--------|-------------|
| **System Tray** | Requires `tauri::tray` | Close window to exit |
| **Deep Link** | Requires `tauri-plugin-deep-link` | Manual import |
| **Auto Update** | Requires `tauri-plugin-updater` | Use apt to manage updates |
| **Native Menu** | Requires Tauri menu system | WebView context menu |

---

## Build Dependencies

### System Dependencies

```bash
# Ubuntu 20.04 — base build dependencies
sudo apt install build-essential pkg-config libssl-dev libsqlite3-dev

# Native window (required for native-shell)
sudo apt install libgtk-3-dev libwebkit2gtk-4.0-dev
```

No need for `libwebkit2gtk-4.1-dev` (Tauri 2 requirement, unavailable on 20.04).

### Rust Toolchain

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# Rust 1.70+ recommended, 1.85+ preferred
```

### Node.js and pnpm (frontend build only)

```bash
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt install nodejs
npm install -g pnpm
```

---

## Build and Run

### Development

```bash
# 1. Build frontend
pnpm install
pnpm build:renderer

# 2. Build web server + native window
cargo build -p web-server
cd native-shell && cargo build && cd ..

# 3. Run native window
./native-shell/target/debug/native-shell
```

### Hot Reload Development

```bash
# Terminal 1: Start backend API server
cargo run -p web-server

# Terminal 2: Start Vite dev server
pnpm dev:renderer
# Access http://localhost:3000?native=1 in browser, API proxied to port 2891
```

---

## Package as .deb

Builds a `.deb` package with two binaries:
- `cc-switch-web` — Native GTK+WebKit desktop window (double-click or run `cc-switch-web` to use)
- `cc-switch-web-server` — Embedded HTTP server (managed by native window)

### Build

```bash
./build-deb.sh
```

Output: `cc-switch-web_1.0.0_amd64.deb`

### Install

```bash
sudo dpkg -i cc-switch-web_1.0.0_amd64.deb
```

Launch CC Switch from your application menu or desktop icon.

### Usage

1. Double-click the desktop icon to open CC Switch
2. Close the window to automatically stop all background services
3. Enable launch on startup in **Settings → Window Behavior → Launch on Startup**

### Uninstall

```bash
sudo dpkg -r cc-switch-web            # keep config
sudo dpkg --purge cc-switch-web       # fully remove
```

---

## Architecture

```
┌─ User double-clicks ─────────────────────────────┐
│  /usr/bin/cc-switch-web (native-shell)          │
│  ┌─ GTK3 Window ─────────────────────────────┐   │
│  │  WebKit2GTK 4.0 WebView                    │   │
│  │  → http://127.0.0.1:2891?native=1         │   │
│  │  (Native title bar, no address bar)        │   │
│  └──────────┬─────────────────────────────────┘   │
│             │ HTTP (loopback)                     │
│  ┌──────────▼─────────────────────────────────┐   │
│  │  cc-switch-web-server (child process)          │   │
│  │  127.0.0.1:2891                             │   │
│  └─────────────────────────────────────────────┘   │
│                                                     │
│  Open window → start server → load UI               │
│  Close window → stop server → exit cleanly          │
└─────────────────────────────────────────────────────┘
```

### Lifecycle

- **Open** → `native-shell` spawns `cc-switch-web-server` as child process → waits for server → opens WebView
- **Use** → Frontend communicates with backend via HTTP API
- **Close** → `native-shell` sends shutdown signal → kills child process → clean exit

---

## Data Storage

Same as original Tauri version:
- **Database**: `~/.cc-switch/cc-switch.db` (SQLite)
- **Settings**: `~/.cc-switch/settings.json`
- **Backups**: `~/.cc-switch/backups/` (auto-rotating, 10 copies)

Fully compatible with the original Tauri desktop version data format.

---

> Original Tauri 2 desktop version by [farion1231](https://github.com/farion1231/cc-switch), released under MIT license.

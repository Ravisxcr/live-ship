# AGENTS.md - Developer & Agent Guide for Live-Ship

Welcome to **Live-Ship** (`live-ship`). This guide provides technical architecture, conventions, constraints, and instructions for AI agents and human developers contributing to this codebase.

---

## 1. Project Mission & Overview

**Live-Ship** is a native, lightweight desktop GUI application written in **Rust** using the **Slint** UI framework. It manages standalone web services, scripts, and local developer applications (such as Python FastAPI, Go binaries, Node/React apps, and shell scripts).

### Core Features
- **Collapsible Sidebar**: Easy navigation between Dashboard, New Deployment form, and quick status overview.
- **Process Supervision**: Launch processes in custom working directories with injected environment variables (`.env` format).
- **Live Terminal Logging**: Capture real-time `stdout` and `stderr` streams into a circular log buffer with timestamped terminal viewing.
- **Color-Coded Status Dashboard**: Clear visual state indication (🟢 Running, 🟡 Starting, 🔴 Error, ⚪ Stopped) with one-click actions (Start, Stop, Restart, View Logs, Edit, Delete).
- **Local Persistence**: Automatically saves configurations to JSON in user application data (`%APPDATA%/liveship` or platform equivalent).

---

## 2. Tech Stack & Dependencies

- **Language**: Rust (Edition 2024, `rustc >= 1.85`)
- **GUI Toolkit**: [Slint](https://slint.dev/) `1.18.1` (`slint` + `slint-build`)
- **Serialization**: `serde` `1.0`, `serde_json` `1.0`
- **ID & Time Generation**: `uuid` `1.10` (v4), `chrono` `0.4`
- **Target OS**: Cross-platform (Windows-first implementation with process-tree cleanup).

---

## 3. Directory Layout & Architecture

The project adheres to a strict modular separation of concerns:

```
live-ship/
├── Cargo.toml                  # Workspace dependencies and Slint build config
├── build.rs                    # Slint compiler build hook (`slint_build::compile`)
├── src/
│   ├── main.rs                 # Application entry point: initializes state, builds UI, runs event loop
│   ├── api/                    # Future headless/REST API endpoints and external interfaces
│   │   └── mod.rs
│   ├── app/                    # Core application state & orchestration
│   │   ├── mod.rs
│   │   ├── manager.rs          # DeploymentManager: starts/stops processes, polls health, ingests logs
│   │   └── state.rs            # ActiveDeployment runtime models
│   ├── config/                 # Persistent configuration models & file I/O
│   │   ├── mod.rs
│   │   ├── deployment.rs       # DeploymentConfig, AppType, AppStatus schemas
│   │   └── storage.rs          # JSON file persistence (%APPDATA%/liveship/deployments.json)
│   ├── ui/                     # Presentation layer (Slint templates + Rust bridges)
│   │   ├── mod.rs
│   │   ├── app_window.slint    # Root window, view state, navigation
│   │   ├── components/
│   │   │   ├── sidebar.slint   # Collapsible navigation drawer
│   │   │   ├── status_badge.slint # Visual status pill with colored indicator dot
│   │   │   └── log_viewer.slint# Terminal-like monospace log streaming console
│   │   ├── views/
│   │   │   ├── dashboard.slint # Main deployment cards grid
│   │   │   └── deployment_form.slint # Creation/Edit form with app presets
│   │   └── handlers/
│   │       ├── mod.rs
│   │       └── bridge.rs       # Rust-to-Slint data synchronization & callback handlers
│   └── utils/                  # Reusable utilities
│       ├── mod.rs
│       ├── env.rs              # .env multiline parser & serializer
│       ├── logger.rs           # LogEntry and ring LogBuffer (2,000 lines capacity)
│       └── process.rs          # Child process spawner, pipe stream reader, tree-killing
```

---

## 4. Key Architectural Patterns

### 4.1 Process Spawning & Windows Process Tree Cleanup
- Spawning child processes with shells (e.g. `cmd.exe /C "npm start"` or `uvicorn`) creates process trees.
- Calling `std::process::Child::kill()` on Windows only terminates the parent shell, leaving orphaned web servers running and blocking ports.
- **Rule**: In `src/utils/process.rs`, child termination on Windows uses `taskkill /F /T /PID <pid>`. Always preserve this tree-killing behavior for clean process exits.

### 4.2 Slint Bridge & UI Synchronization
- Slint data models (`ModelRc<VecModel<T>>`) require thread-safe updates on the main UI thread.
- `src/ui/handlers/bridge.rs` sets up a 250ms polling timer via `slint::Timer`.
- In each tick:
  - `manager.tick()` checks child process exit statuses, restarts crashing processes if `auto_restart` is set, and drains background logs.
  - Active deployment states are mapped into Slint `DeploymentCardData` and `DeploymentNavItem` structs and synced back into UI models.

### 4.3 Environment Variable Parsing
- `src/utils/env.rs` parses standard `.env` formats:
  - Supports `KEY=VALUE`, `export KEY=VALUE`.
  - Strips surrounding single or double quotes.
  - Skips empty lines and comments (`#`).

---

## 5. Slint UI Guidelines & Gotchas

When writing or editing `.slint` files, strictly observe these rules:

1. **No Directional Borders on Rectangles**:
   - `border-bottom-width`, `border-top-width`, etc., are **invalid** in Slint.
   - Only `border-width` is supported.
   - For 1px dividers, use a dedicated `Rectangle { height: 1px; background: #...; }`.
2. **Padding Constraints**:
   - `padding`, `padding-left`, `padding-top`, etc., can **only** be placed on layout containers (`HorizontalLayout`, `VerticalLayout`, `GridLayout`), **never** directly on `Rectangle` or `Text`.
3. **Rust Imports for Slint**:
   - Always ensure `use slint::ComponentHandle;` is in scope in any Rust module calling `.clone_strong()`, `.run()`, or accessing window methods.
4. **Model Updates**:
   - To update a Slint array property, build a `slint::VecModel`, wrap it with `slint::ModelRc::new(...)`, and assign it to the property.

---

## 6. Build, Test, and Run Commands

Always verify your changes with the following commands before finalizing:

### Check Syntax & Slint Compilation
```powershell
cargo check
```
*(This triggers `build.rs` and checks all Slint templates for syntax and type errors).*

### Run Unit Tests
```powershell
cargo test
```
*(Tests cover JSON storage persistence, `.env` parsing/formatting, and deployment lifecycle orchestration).*

### Build Debug / Release
```powershell
# Debug build:
cargo build

# Release build:
cargo build --release
```

### Launch the Application
```powershell
cargo run
```

---

## 7. Roadmap & Planned Enhancements

When implementing new features, refer to these prioritized items:

1. **Native Directory Picker**:
   - Integrate `rfd` (Rust File Dialog) to allow browsing for working directories via a native OS dialog.
2. **Health Check & Port Probing**:
   - Automatically extract `PORT` from `.env` or process arguments and probe `http://localhost:<PORT>` with an HTTP HEAD request; display clickable links in the dashboard card.
3. **Log Search & Export**:
   - Add text search filter in `log_viewer.slint` and an "Export Logs to File" button.
4. **System Tray Integration**:
   - Allow minimizing the window to the system tray while keeping supervised applications running in the background.

---

## 8. Agent Instructions & Code Quality Checklist

- **Documentation Integrity**: Maintain comments and docstrings.
- **Error Handling**: Prefer `Result<T, AppError>` and graceful fallback logging over `unwrap()` / `expect()` in runtime code.
- **Thread Safety**: Never hold a `MutexGuard` across long-running operations or UI event handlers.
- **Verification**: Run `cargo check` and `cargo test` after any code modification.


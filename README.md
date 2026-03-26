# Differential Synchronization in Rust

A Rust implementation of Neil Fraser's [Differential Synchronization](https://neil.fraser.name/writing/sync/) algorithm for real-time collaborative document editing, with a browser-based editor frontend.

## Overview

Real-time collaborative editing using differential synchronization. Multiple users can edit the same document simultaneously from the browser or CLI — edits are synchronized every 500ms with automatic conflict resolution.

## Quick Start

```bash
# Build the frontend
cd web && npm install && npm run build && cd ..

# Start the server (serves both the editor UI and WebSocket on :8081)
cargo run --bin ws-server --release

# Open http://localhost:8081 in multiple browser tabs
```

### CLI Clients

```bash
# TCP server runs on :8080 alongside WebSocket
cargo run --bin client --release -- --client-id alice
cargo run --bin client --release -- --client-id bob
```

## Features

- **Browser-based editor** — Next.js frontend with real-time sync
- **Remote cursor tracking** — See where other users are editing with colored name tags
- **Automatic conflict resolution** — Fraser's algorithm handles concurrent edits
- **Persistent storage** — SQLite database with document versioning
- **Dual transport** — WebSocket for browsers, TCP for CLI clients
- **Single-port deployment** — Static files and WebSocket served from the same origin

## Project Structure

```
src/
  diff.rs          # Diff/patch algorithm (Edit, EditList)
  sync.rs          # SyncEngine (document + shadow management)
  document.rs      # Versioned document model
  network.rs       # Wire protocol, SyncServer, message handling
  persistence.rs   # SQLite storage (DocumentDB)
  bin/
    ws_server.rs   # Production server (TCP + WebSocket + static files)
    server.rs      # TCP-only server
    client.rs      # TCP CLI client
    demo.rs        # Local simulation (interactive, benchmark)
web/
  app/page.tsx     # Editor UI
  lib/
    use-sync.ts    # WebSocket connection + sync hook
    sync-engine.ts # TypeScript SyncEngine (mirrors Rust)
    diff.ts        # TypeScript diff/patch (byte-offset compatible)
    protocol.ts    # Message type definitions
    cursor-overlay.tsx  # Remote cursor rendering
test/
  e2e/             # Playwright browser tests
```

## Demos

```bash
# Interactive two-user simulation
cargo run --bin sync-demo interactive

# Automated concurrent edit simulation
cargo run --bin sync-demo simulate --iterations 5

# Performance benchmark
cargo run --bin sync-demo benchmark --iterations 1000
```

## Testing

```bash
# Rust unit tests
cargo test

# Browser e2e tests (starts server automatically)
cd test && npm install && npx playwright install && npx playwright test
```

## Documentation

- **[System Architecture](docs/ARCHITECTURE.md)** — Design, data flow, and module structure

## Resources

- [Original Paper](https://neil.fraser.name/writing/sync/) by Neil Fraser
- [Diff/Patch Demo](http://neil.fraser.name/software/diff_match_patch/demo_patch.html) by Neil Fraser

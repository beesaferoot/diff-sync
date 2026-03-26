# System Architecture

## Overview

A real-time collaborative editor built on Neil Fraser's [Differential Synchronization](https://neil.fraser.name/writing/sync/) algorithm. A Rust server handles document persistence and multi-client synchronization over both TCP and WebSocket, while a Next.js frontend provides the browser-based editing interface.

## System Topology

```mermaid
graph TB
    subgraph Clients
        WA[Browser Client A]
        WB[Browser Client B]
        TC[TCP CLI Client]
    end

    subgraph "Rust Server (ws-server)"
        AX[Axum HTTP + WS]
        TL[TCP Listener]
        SS[SyncServer]
        SF[Static File Server]
    end

    DB[(SQLite<br/>documents.db)]

    WA -- "WebSocket :8081/ws" --> AX
    WB -- "WebSocket :8081/ws" --> AX
    WA -- "HTTP GET :8081/" --> SF
    WB -- "HTTP GET :8081/" --> SF
    TC -- "TCP :8080" --> TL
    AX --> SS
    TL --> SS
    SS --> DB
```

The `ws-server` binary runs two listeners on separate ports. Axum serves both the Next.js static export and WebSocket upgrades on `:8081`. A plain TCP listener on `:8080` supports the CLI client. Both transports share a single `SyncServer` instance behind `Arc<Mutex<_>>`.

## Core Algorithm: Dual-Shadow Sync

Each client maintains a **document** (working copy) and a **shadow** (last agreed state with the server). The server maintains a per-client shadow to track what each client has seen.

```mermaid
sequenceDiagram
    participant C as Client
    participant S as Server
    participant DB as SQLite

    C->>S: Connect { client_id }
    S->>DB: Load document
    DB-->>S: Document content + version
    S->>S: Create ClientSession with shadow = document
    S-->>C: ConnectOk { document, server_version }

    loop Every 500ms
        C->>C: edits = diff(shadow, document)
        C->>C: shadow = document
        C->>S: ClientSync { edits, cursor_position }
        S->>S: patch(db_document, client_edits)
        S->>DB: Save updated document
        S->>S: Update client shadow
        S->>S: server_edits = diff(client_shadow, db_document)
        S->>S: Update client shadow to db_document
        S-->>C: ServerSync { server_edits, cursors }
        C->>C: patch(shadow, server_edits)
        C->>C: patch(document, server_edits)
    end
```

The key insight: the server diffs each client's shadow against the current DB document. This produces edits containing only changes from *other* clients, since the requesting client's own edits have already been applied.

## Module Structure

```mermaid
graph LR
    subgraph "Library (src/)"
        diff[diff.rs<br/>Edit, EditList, diff, patch]
        doc[document.rs<br/>Document]
        sync[sync.rs<br/>SyncEngine]
        net[network.rs<br/>SyncMessage, SyncServer<br/>handle_sync_message]
        persist[persistence.rs<br/>DocumentDB]
    end

    subgraph "Binaries (src/bin/)"
        ws[ws_server.rs<br/>TCP + WS + static files]
        srv[server.rs<br/>TCP only]
        cli[client.rs<br/>TCP CLI]
        demo[demo.rs<br/>Local simulation]
    end

    subgraph "Frontend (web/)"
        page[app/page.tsx<br/>Editor UI]
        hook[lib/use-sync.ts<br/>WebSocket hook]
        tsync[lib/sync-engine.ts<br/>TS SyncEngine]
        tdiff[lib/diff.ts<br/>TS diff/patch]
        overlay[lib/cursor-overlay.tsx<br/>Remote cursors]
        proto[lib/protocol.ts<br/>Message types]
    end

    sync --> diff
    sync --> doc
    net --> sync
    net --> persist
    net --> diff
    ws --> net
    srv --> net
    cli --> net
    hook --> tsync
    tsync --> tdiff
    hook --> proto
    page --> hook
    page --> overlay
```

## Wire Protocol

Messages are serialized as externally-tagged JSON (serde default). TCP uses newline-delimited JSON; WebSocket uses one message per frame.

| Message | Direction | Purpose |
|---------|-----------|---------|
| `Connect` | Client → Server | Join session with a `client_id` |
| `ConnectOk` | Server → Client | Confirm connection, send current document |
| `ClientSync` | Client → Server | Send local edits + cursor position |
| `ServerSync` | Server → Client | Return other clients' edits + all remote cursors |
| `Ping` / `Pong` | Both | Keepalive (30s interval on TCP) |
| `Disconnect` | Client → Server | Leave session |
| `Error` | Server → Client | Error response |

### Cursor Tracking

Cursor positions piggyback on the existing sync cycle — no separate message type or broadcast channel required. Each `ClientSync` includes an optional `cursor_position`. The server stores it per session and returns all other clients' cursors (with assigned colors) in every `ServerSync` response. This gives ~500ms cursor update latency.

## Diff/Patch Engine

Both Rust (`src/diff.rs`) and TypeScript (`web/lib/diff.ts`) implement the same algorithm:

1. **diff(from, to)**: Strip common prefix and suffix, emit a single `Insert`, `Delete`, or `Replace` for the differing middle
2. **patch(text, edits)**: Apply edits in reverse order to avoid position shifts; clamp positions to bounds for fuzzy tolerance

All positions are **byte offsets** (Rust strings are UTF-8 byte arrays). The TypeScript side uses `TextEncoder`/`TextDecoder` to produce compatible offsets. Each `EditList` carries a checksum of the source text for validation.

## Persistence

SQLite stores documents with content, version number, and timestamps. The server reads the document on each sync cycle and writes back after applying client edits. The `DocumentDB` initializes with a default document on first run.

## Frontend Architecture

The Next.js app is built as a static export (`output: "export"`) and served directly by the Rust server — no separate Node.js process in production.

```mermaid
graph TB
    subgraph "Browser"
        UI[EditorPage] --> |"onChange"| Hook[useSync hook]
        UI --> |"cursors prop"| Overlay[CursorOverlay]
        Hook --> |"WebSocket"| Engine[SyncEngine]
        Engine --> Diff[diff / patch]
        Hook --> |"remoteCursors state"| UI
    end

    Hook -- "ClientSync every 500ms" --> Server[Rust WS Server]
    Server -- "ServerSync" --> Hook
```

The `useSync` hook manages the full lifecycle: WebSocket connection, reconnection with exponential backoff, 500ms sync interval, and cursor position tracking. The `CursorOverlay` renders remote user name tags at cursor positions using a transparent overlay div mirroring the textarea's font and layout.

## Deployment

For local development:
```
cargo run --bin ws-server    # serves everything on :8081
```

For remote access (e.g., mobile via ngrok):
```
ngrok http 8081              # single tunnel covers both static files and WebSocket
```

The frontend derives the WebSocket URL from `window.location`, so it works transparently behind reverse proxies and HTTPS tunnels.

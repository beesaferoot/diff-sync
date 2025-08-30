# Differential Synchronization in Rust

A Rust implementation of Neil Fraser's [Differential Synchronization](https://neil.fraser.name/writing/sync/) algorithm for real-time collaborative document editing.

## 🎯 Project Overview

This project demonstrates **real-time collaborative editing** using Neil Fraser's differential synchronization algorithm. It's designed for weekend hackers who want to:

- **Learn Rust** through practical async networking and algorithms
- **Understand differential sync** by implementing it from scratch
- **Build real-time collaboration** with TCP client/server architecture
- **Experiment with persistence** using SQLite databases

## 🚀 Quick Start

```bash
# Start collaborative editing server
cargo run --bin server --release

# Connect multiple clients
cargo run --bin client --release -- --client-id alice
cargo run --bin client --release -- --client-id bob
```

## ✨ Key Features

- **Real-time collaboration** - Multiple users editing simultaneously
- **Automatic conflict resolution** - Fraser's algorithm handles concurrent edits
- **Persistent storage** - SQLite database with version tracking
- **Network layer** - TCP client/server with async Tokio
- **Live synchronization** - 500ms sync cycles for responsiveness

## 📚 Documentation

- **[System Architecture](docs/ARCHITECTURE.md)** - High-level design with ASCII diagrams
- **[Persistence Layer](docs/PERSISTENCE_SUMMARY.md)** - SQLite integration details

## 🧪 Learning Value

Perfect for learning:
- **Rust async/await** with Tokio
- **Network programming** with TCP streams
- **Algorithm implementation** from academic papers
- **Database integration** with SQLite
- **Real-time systems** design patterns

## Quick Start

### Local Demos
```bash
# Interactive demo - play with two users editing simultaneously
cargo run --bin sync-demo interactive

# Simulation of concurrent edits
cargo run --bin sync-demo simulate --iterations 5

# Benchmark performance
cargo run --bin sync-demo benchmark --iterations 1000
```

### Network Demo
```bash
# Terminal 1: Start server
cargo run --bin server

# Terminal 2: Connect first client
cargo run --bin client -- --client-id alice

# Terminal 3: Connect second client  
cargo run --bin client -- --client-id bob

# Custom server address
cargo run --bin server -- --address 0.0.0.0:9090
cargo run --bin client -- --server 127.0.0.1:9090 --client-id charlie
```

### Live Collaborative Editing Demo
```bash
# Terminal 1: Start server with persistence
cargo run --bin server --release

# Terminal 2: Alice joins
cargo run --bin client --release -- --client-id alice
# In Alice's client: edit Hello from Alice!

# Terminal 3: Bob joins (sees Alice's message instantly!)  
cargo run --bin client --release -- --client-id bob
# In Bob's client: edit Hi Alice, this is Bob!

# Watch Alice's terminal - Bob's message appears automatically!
# 🌍 LIVE UPDATE: 1 edits from other users!
# 📄 Document: "Hi Alice, this is Bob!"
```

### Persistence Demo 
```bash
# Terminal 1: Start server with custom database
cargo run --bin server --release --database-path my_docs.db --document-name shared_doc

# Terminal 2: Alice edits
cargo run --bin client --release -- --client-id alice
# edit Welcome to persistent collaborative editing!
# quit

# Stop server (Ctrl+C), then restart
cargo run --bin server --release --database-path my_docs.db --document-name shared_doc

# Terminal 3: Bob connects after restart
cargo run --bin client --release -- --client-id bob
# Shows: "Welcome to persistent collaborative editing!" (Alice's edit persisted!)
```

## Interactive Demo Commands

- `a <text>` - Edit Alice's document
- `b <text>` - Edit Bob's document  
- `s` - Synchronize documents
- `h` - Show help
- `q` - Quit

Example session:
```
> a The quick brown fox
✏️ Alice edited document

> b The lazy dog sleeps
✏️ Bob edited document

> s
=== Synchronizing ===
✅ Documents are synchronized!
```

## Features Implemented

- ✅ **Basic dual-shadow synchronization** - Core algorithm working
- ✅ **Fuzzy patching** - Handles concurrent edits gracefully  
- ✅ **Interactive demo** - See the algorithm in action
- ✅ **Simulation mode** - Test with predefined edit sequences
- ✅ **Benchmarking** - Performance measurement (39,000+ syncs/second!)
- ✅ **Network layer** - TCP client/server with async tokio
- ✅ **Multiple clients** - Server supporting concurrent connections
- ✅ **Protocol design** - JSON message-based communication  
- ✅ **Interactive editing** - Real-time collaborative text editing UI
- ✅ **Live synchronization** - Continuous document sync between all clients (500ms)
- ✅ **Stable connections** - Heartbeat mechanism prevents random disconnects
- ✅ **SQLite persistence** - Documents persist across server restarts with version tracking
- ✅ **Unified initial state** - All clients start with the same document from database
- 🔄 **Guaranteed delivery** - Version tracking and backup shadows (next step)

## Next Steps for Weekend Hacking

1. ✅ **Run the demo** - Get familiar with the algorithm
2. ✅ **Add network layer** - TCP client/server working!
3. ✅ **Interactive editing** - Real-time text editing commands  
4. ✅ **Live synchronization** - Continuous collaborative editing
5. ✅ **Document persistence** - SQLite database with version tracking
6. **Guaranteed delivery** - Add version tracking and backup shadows
7. **Web interface** - Browser-based collaborative editor (WebSocket + WASM)
8. **Multi-document support** - Support multiple named documents in one server
9. **Advanced diff** - Implement Fraser's full algorithm optimizations
10. **Conflict visualization** - Show merge conflicts and resolution
11. **User awareness** - Show who's currently editing what
12. **Document history** - View and restore previous versions

## Learning Value

This project demonstrates several important concepts:

- **Distributed systems** - Handling concurrent modifications
- **Conflict resolution** - Fuzzy patching and automatic recovery
- **State management** - Shadow copies and version tracking
- **Network programming** - Client/server architecture
- **Real-time systems** - Responsive collaboration despite latency

## Key Insights from Implementation

1. **The shadow is crucial** - It provides a stable reference point for diffs
2. **Fuzzy patching works** - Most edits apply successfully even to changed text
3. **Self-healing is powerful** - Failed patches get corrected automatically
4. **Order matters** - Applying edits in the right sequence is important
5. **Performance scales** - The algorithm handles large documents efficiently

## Resources

- [Original Paper](https://neil.fraser.name/writing/sync/) by Neil Fraser
- [Live Demo](http://neil.fraser.name/software/diff_match_patch/demo_patch.html) of diff/patch
- [Google Wave](https://en.wikipedia.org/wiki/Apache_Wave) - Real-world application

Perfect for understanding distributed systems, conflict resolution, and building something that actually works!

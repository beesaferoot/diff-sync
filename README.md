# Differential Synchronization in Rust

A Rust implementation of Neil Fraser's [Differential Synchronization](https://neil.fraser.name/writing/sync/) algorithm for real-time collaborative document editing.

## 🎯 Overview

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

## ✨ Features

- **Real-time collaboration** - Multiple users editing simultaneously
- **Automatic conflict resolution** - Fraser's algorithm handles concurrent edits
- **Persistent storage** - SQLite database with version tracking
- **Network layer** - TCP client/server with async Tokio
- **Live synchronization** - 500ms sync cycles for responsiveness

## 🧪 Demos

### Interactive Demo
```bash
# Play with two users editing simultaneously
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
```

### Live Collaborative Editing
```bash
# Terminal 1: Start server with persistence
cargo run --bin server --release

# Terminal 2: Alice joins and edits
cargo run --bin client --release -- --client-id alice
# Edit: Hello from Alice!

# Terminal 3: Bob joins (sees Alice's message instantly!)  
cargo run --bin client --release -- --client-id bob
# Edit: Hi Alice, this is Bob!

# Watch Alice's terminal - Bob's message appears automatically!
```

## 📚 Documentation

- **[System Architecture](docs/ARCHITECTURE.md)** - High-level design with ASCII diagrams

## 🎓 Learning Value

Perfect for learning:
- **Rust async/await** with Tokio
- **Network programming** with TCP streams
- **Algorithm implementation** from academic papers
- **Database integration** with SQLite
- **Real-time systems** design patterns

## 🚧 Next Steps

1. **Guaranteed delivery** - Add version tracking and backup shadows
2. **Web interface** - Browser-based collaborative editor (WebSocket + WASM)
3. **Multi-document support** - Support multiple named documents
4. **Advanced diff** - Implement Fraser's full algorithm optimizations
5. **Conflict visualization** - Show merge conflicts and resolution

## 📖 Resources

- [Original Paper](https://neil.fraser.name/writing/sync/) by Neil Fraser
- [Live Demo](http://neil.fraser.name/software/diff_match_patch/demo_patch.html) of diff/patch
- [Google Wave](https://en.wikipedia.org/wiki/Apache_Wave) - Real-world application

Perfect for understanding distributed systems, conflict resolution, and building something that actually works!

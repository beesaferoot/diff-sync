# 🏗️ System Architecture

## Overview

The Differential Synchronization system implements Neil Fraser's algorithm for real-time collaborative editing. This document shows the high-level design and how components interact.

## 🎯 Core Concept: Dual Shadow Synchronization

```
┌─────────────────────────────────────────────────────────────────┐
│                    DIFFERENTIAL SYNCHRONIZATION                 │
│                                                                 │
│  Each client maintains TWO copies of the document:              │
│  • Document: Current working copy                               │
│  • Shadow: Server's last known state                            │
│                                                                 │
│  Changes flow: Document ↔ Shadow ↔ Server ↔ Other Clients       │
└─────────────────────────────────────────────────────────────────┘
```

## 🏛️ High-Level System Architecture

```
                    ┌─────────────────────────────────────────┐
                    │              CLIENTS                    │
                    │                                         │
                    │  ┌─────────────┐  ┌─────────────┐       │
                    │  │   Alice     │  │    Bob      │       │
                    │  │             │  │             │       │
                    │  │ Document    │  │ Document    │       │
                    │  │ + Shadow    │  │ + Shadow    │       │
                    │  └─────────────┘  └─────────────┘       │
                    └─────────────────────────────────────────┘
                                    │
                                    │ TCP/JSON
                                    │
                    ┌─────────────────────────────────────────┐
                    │               SERVER                    │
                    │                                         │
                    │  ┌─────────────────────────────────────┐│
                    │  │         SyncServer                  ││
                    │  │                                     ││
                    │  │  ┌─────────────┐  ┌─────────────┐   ││
                    │  │  │ Client      │  │ Client      │   ││
                    │  │  │ Sessions    │  │ Sessions    │   ││
                    │  │  │ (Shadows)   │  │ (Shadows)   │   ││
                    │  │  └─────────────┘  └─────────────┘    │
                    │  └─────────────────────────────────────┘│
                    └─────────────────────────────────────────┘
                                    │
                                    │ SQLite
                                    │
                    ┌─────────────────────────────────────────┐
                    │              DATABASE                   │
                    │                                         │
                    │  ┌─────────────────────────────────────┐│
                    │  │         documents.db                ││
                    │  │                                     ││
                    │  │  ┌─────────────┐  ┌─────────────┐   ││
                    │  │  │ Document    │  │ Document    │   ││
                    │  │  │ + Version   │  │ + Version   │   ││
                    │  │  │ + Timestamp │  │ + Timestamp │   ││
                    │  │  └─────────────┘  └─────────────┘   ││
                    │  └─────────────────────────────────────┘│
                    └─────────────────────────────────────────┘
```

## 🔄 How Components Connect

### **1. Client Structure**
```
┌─────────────────────────────────────────────────────────────┐
│                        CLIENT                               │
│                                                             │
│  ┌─────────────────┐    ┌─────────────────┐                 │
│  │   User Input    │    │  Network I/O    │                 │
│  │                 │    │                 │                 │
│  │  • edit text    │    │  • TCP Stream   │                 │
│  │  • show doc     │    │  • JSON Msgs    │                 │
│  │  • quit         │    │  • Heartbeat    │                 │
│  └─────────────────┘    └─────────────────┘                 │
│           │                       │                         │
│           ▼                       ▼                         │
│  ┌─────────────────────────────────────────────────────┐    │
│  │              SyncEngine                             │    │
│  │                                                     │    │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │    │
│  │  │ Document    │  │   Shadow    │  │  Pending    │  │    │
│  │  │ (Current)   │  │ (Server)    │  │   Edits     │  │    │
│  │  └─────────────┘  └─────────────┘  └─────────────┘  │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

### **2. Server Structure**
```
┌─────────────────────────────────────────────────────────────┐
│                        SERVER                               │
│                                                             │
│  ┌─────────────────┐    ┌─────────────────┐                 │
│  │   TCP Listener  │    │  Document DB    │                 │
│  │                 │    │                 │                 │
│  │  • Accept       │    │  • SQLite       │                 │
│  │  • Route        │    │  • ACID         │                 │
│  │  • Manage       │    │  • Versioning   │                 │
│  └─────────────────┘    └─────────────────┘                 │
│           │                       │                         │
│           ▼                       ▼                         │
│  ┌─────────────────────────────────────────────────────┐    │
│  │              SyncServer                             │    │
│  │                                                     │    │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │    │
│  │  │   Master    │  │   Client    │  │   Client    │  │    │
│  │  │ Document    │  │  Session A  │  │  Session B  │  │    │
│  │  │ (Database)  │  │  (Shadow)   │  │  (Shadow)   │  │    │
│  │  └─────────────┘  └─────────────┘  └─────────────┘  │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

## 🔄 Data Flow Diagrams

### **Client Connection Flow**
```
┌─────────┐    Connect    ┌─────────┐    Load Doc   ┌─────────┐
│ Client  │ ────────────→ │ Server  │ ────────────→ │Database │
│         │               │         │               │         │
└─────────┘               └─────────┘               └─────────┘
     │                         │                         │
     │                         ▼                         │
     │               ┌─────────────────┐                 │
     │               │ Create Session  │                 │
     │               │ + Initialize    │                 │
     │               │   Shadow        │                 │
     │               └─────────────────┘                 │
     │                         │                         │
     ▼                         ▼                         │
┌─────────┐    Document   ┌─────────┐                    │
│ Client  │ ←─────────────│ Server  │                    │
│ Shadow  │               │         │                    │
└─────────┘               └─────────┘                    │
```

### **Edit Synchronization Flow**
```
┌─────────┐    Edit       ┌─────────┐    Save to    ┌─────────┐
│ Client  │ ────────────→ │ Server  │ ────────────→ │Database │
│ Alice   │               │         │               │         │
└─────────┘               └─────────┘               └─────────┘
     │                         │                         │
     │                         ▼                         │
     │               ┌─────────────────┐                 │
     │               │ Update Alice's  │                 │
     │               │   Shadow        │                 │
     │               └─────────────────┘                 │
     │                         │                         │
     │                         ▼                         │
     │               ┌─────────────────┐                 │
     │               │ Generate Diffs  │                 │
     │               │ for Other       │                 │
     │               │   Clients       │                 │
     │               └─────────────────┘                 │
     │                         │                         │
     ▼                         ▼                         │
┌─────────┐    Updates    ┌─────────┐                    │
│ Client  │ ←─────────────│ Server  │                    │
│ Bob     │               │         │                    │
└─────────┘               └─────────┘                    │
```

### **Live Sync Cycle (Every 500ms)**
```
┌─────────────────────────────────────────────────────────────┐
│                    LIVE SYNC CYCLE                          │
│                                                             │
│  ┌─────────┐    ┌─────────┐    ┌─────────┐                  │
│  │ Client  │    │ Server  │    │Database │                  │
│  │         │    │         │    │         │                  │
│  │ 1. Gen  │    │ 3. Proc │    │ 4. Save │                  │
│  │   Diff  │    │  Edits  │    │ Changes │                  │
│  └─────────┘    └─────────┘    └─────────┘                  │
│       │              │              │                       │
│       ▼              ▼              ▼                       │
│  ┌─────────┐    ┌─────────┐    ┌─────────┐                  │
│  │ Client  │    │ Server  │    │Database │                  │
│  │         │    │         │    │         │                  │
│  │ 2. Send │    │ 5. Gen  │    │ 6. Load │                  │
│  │   Diff  │    │  Diffs  │    │  State  │                  │
│  └─────────┘    └─────────┘    └─────────┘                  │
│       │              │              │                       │
│       ▼              ▼              ▼                       │
│  ┌─────────┐    ┌─────────┐    ┌─────────┐                  │
│  │ Client  │    │ Server  │    │Database │                  │
│  │         │    │         │    │         │                  │
│  │ 7. Recv │    │ 8. Send │    │         │                  │
│  │ Updates │    │ Updates │    │         │                  │
│  └─────────┘    └─────────┘    └─────────┘                  │
└─────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
                            ┌─────────────┐
                            │   Wait      │
                            │  500ms      │
                            └─────────────┘
                                    │
                                    ▼
                            ┌─────────────┐
                            │   Repeat    │
                            └─────────────┘
```

## 🌐 Network Protocol Overview

### **Message Flow**
```
┌─────────┐    Connect    ┌─────────┐
│ Client  │ ────────────→ │ Server  │
│         │               │         │
└─────────┘               └─────────┘
     │                         │
     │                         ▼
     │               ┌─────────────────┐
     │               │ Load Document   │
     │               │ from Database   │
     │               └─────────────────┘
     │                         │
     ▼                         ▼
┌─────────┐    ConnectOk  ┌─────────┐
│ Client  │ ←─────────────│ Server  │
│         │               │         │
└─────────┘               └─────────┘
     │                         │
     │                         │ 500ms intervals
     │                         │
     ▼                         ▼
┌─────────┐    ClientSync ┌─────────┐
│ Client  │ ────────────→ │ Server  │
│         │               │         │
└─────────┘               └─────────┘
     │                         │
     │                         ▼
     │               ┌─────────────────┐
     │               │ Process Edits   │
     │               │ + Update DB     │
     │               └─────────────────┘
     │                         │
     ▼                         ▼
┌─────────┐    ServerSync ┌─────────┐
│ Client  │ ←─────────────│ Server  │
│         │               │         │
└─────────┘               └─────────┘
```

## 🔧 Key Design Patterns

### **1. Shadow Synchronization**
```
┌─────────────────────────────────────────────────────────────┐
│                    SHADOW PATTERN                           │
│                                                             │
│  Client A:  [Document] ←→ [Shadow] ←→ [Server]              │
│                                                             │
│  Client B:  [Document] ←→ [Shadow] ←→ [Server]              │
│                                                             │
│  • Shadow = Server's last known state                       │
│  • Changes = Diff(Document, Shadow)                         │
│  • Sync = Update Shadow to match Server                     │
└─────────────────────────────────────────────────────────────┘
```

### **2. Continuous Sync Loop**
```
┌────────────────────────────────────────────────────────────┐
│                    SYNC LOOP                               │
│                                                            │
│  ┌─────────┐    ┌─────────┐    ┌─────────┐                 │
│  │ Client  │    │ Server  │    │Database │                 │
│  │         │    │         │    │         │                 │
│  │ Generate│    │ Process │    │ Save    │                 │
│  │   Diff  │    │  Edits  │    │Changes  │                 │
│  └─────────┘    └─────────┘    └─────────┘                 │
│       │              │              │                      │
│       ▼              ▼              ▼                      │
│  ┌─────────┐    ┌─────────┐    ┌─────────┐                 │
│  │ Send    │    │ Generate│    │ Load    │                 │
│  │  Diff   │    │  Diffs  │    │ State   │                 │
│  └─────────┘    └─────────┘    └─────────┘                 │
│       │              │              │                      │
│       ▼              ▼              ▼                      │
│  ┌─────────┐    ┌─────────┐    ┌─────────┐                 │
│  │ Receive │    │ Send    │    │         │                 │
│  │ Updates │    │ Updates │    │         │                 │
│  └─────────┘    └─────────┘    └─────────┘                 │
│       │              │              │                      │
│       └──────────────┼──────────────┘                      │
│                      ▼                                     │
│               ┌─────────────┐                              │
│               │   Wait      │                              │
│               │  500ms      │                              │
│               └─────────────┘                              │
│                      │                                     │
│                      ▼                                     │
│               ┌─────────────┐                              │
│               │   Repeat    │                              │
│               └─────────────┘                              │
└────────────────────────────────────────────────────────────┘
```

## 🚀 System Characteristics

### **Performance**
- **Local sync**: 39,000+ ops/sec
- **Network sync**: 500ms intervals
- **Database**: ACID transactions
- **Memory**: Minimal overhead

### **Scalability**
- **Clients**: Limited by system resources
- **Documents**: Efficient diff algorithms
- **Concurrency**: Fraser's algorithm handles conflicts
- **Network**: Compressed diffs

### **Reliability**
- **Heartbeats**: 30s intervals
- **Checksums**: Edit validation
- **Backup shadows**: Recovery mechanism
- **Graceful degradation**: Timeout handling

## 🔮 Future Extensions

```
┌───────────────────────────────────────────────────────────┐
│                    FUTURE VISION                          │
│                                                           │
│  ┌─────────┐    ┌─────────┐    ┌─────────┐                │
│  │ Web UI  │    │ REST    │    │ Multi-  │                │
│  │ (WASM)  │    │ API     │    │ Document│                │
│  └─────────┘    └─────────┘    └─────────┘                │
│                                                           │
│  ┌─────────┐    ┌─────────┐    ┌─────────┐                │
│  │ Load    │    │ Redis   │    │ Document│                │
│  │ Balance │    │ Cache   │    │ History │                │
│  └─────────┘    └─────────┘    └─────────┘                │
└───────────────────────────────────────────────────────────┘
```

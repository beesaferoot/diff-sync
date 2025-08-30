use clap::Parser;
use colored::*;
use diff_sync::{deserialize_message, serialize_message, SyncEngine, SyncMessage};
use std::io::{self, Write};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration, timeout};

#[derive(Parser)]
#[command(name = "sync-client")]
#[command(about = "Differential synchronization client")]
struct Cli {
    #[arg(short, long, default_value = "127.0.0.1:8080")]
    server: String,
    
    #[arg(short, long)]
    client_id: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    let client_id = cli.client_id.unwrap_or_else(|| {
        format!("client_{}", rand::random::<u32>())
    });
    
    println!("{}", "🚀 Starting Differential Sync Client".blue().bold());
    println!("🌐 Server: {}", cli.server.cyan());
    println!("👤 Client ID: {}", client_id.green());
    
    // Connect to server
    let stream = TcpStream::connect(&cli.server).await?;
    println!("✅ Connected to server");
    
    // Split stream for concurrent read/write
    let (read_half, write_half) = stream.into_split();
    
    // Send connection request to fetch current document
    let connect_msg = SyncMessage::Connect {
        client_id: client_id.clone(),
    };
    
    let mut write_stream = write_half;
    send_message(&mut write_stream, &connect_msg).await?;
    
    // Wait for connection confirmation and document
    let mut reader = BufReader::new(read_half);
    let sync_engine = match receive_message(&mut reader).await? {
        Some(SyncMessage::ConnectOk { server_version, document }) => {
            println!("🤝 Connected to server (version {})", server_version.to_string().cyan());
            println!("📄 Initial document: \"{}\"", document.content.blue());
            
            // Create shared sync engine with document from server
            let mut sync_engine = SyncEngine::new(document.content.clone());
            sync_engine.node_id = client_id.clone();
            Arc::new(Mutex::new(sync_engine))
        }
        Some(SyncMessage::Error { message }) => {
            eprintln!("❌ Connection failed: {}", message.red());
            return Ok(());
        }
        Some(_) => {
            eprintln!("❌ Unexpected response from server");
            return Ok(());
        }
        None => {
            eprintln!("❌ Connection timeout during handshake");
            return Ok(());
        }
    };

    
    // Start background sync task
    let sync_engine_clone = Arc::clone(&sync_engine);
    let client_id_clone = client_id.clone();
    let sync_task = tokio::spawn(async move {
        if let Err(e) = background_sync_task(write_stream, reader, sync_engine_clone, client_id_clone).await {
            eprintln!("❌ Sync task error: {}", e);
        }
    });
    
    // Start interactive command loop
    let sync_engine_clone = Arc::clone(&sync_engine);
    let interactive_task = tokio::spawn(async move {
        interactive_command_loop(sync_engine_clone, client_id).await
    });
    
    // Wait for either task to complete
    tokio::select! {
        _ = sync_task => println!("🔌 Sync task ended"),
        _ = interactive_task => println!("💬 Interactive session ended"),
    }
    
    Ok(())
}

async fn background_sync_task(
    mut write_stream: tokio::net::tcp::OwnedWriteHalf,
    mut reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    sync_engine: Arc<Mutex<SyncEngine>>,
    client_id: String,
) -> Result<(), String> {
    let mut sync_timer = interval(Duration::from_millis(500)); // Sync every 500ms for responsiveness
    let mut heartbeat_timer = interval(Duration::from_secs(30)); // Heartbeat every 30 seconds
    
    loop {
        tokio::select! {
            // Periodic sync - ALWAYS sync to get updates from other clients
            _ = sync_timer.tick() => {
                let (edits, client_version) = {
                    let mut engine = sync_engine.lock().await;
                    let edits = engine.diff_and_update_shadow();
                    let version = engine.document().version;
                    (edits, version)
                };
                
                // CRITICAL: Always send sync request (even with empty edits)
                // This allows receiving updates from other clients
                let sync_msg = SyncMessage::ClientSync {
                    client_id: client_id.clone(),
                    edits,
                    client_version,
                };
                
                if let Err(e) = send_message(&mut write_stream, &sync_msg).await {
                    eprintln!("❌ Failed to send sync: {}", e);
                    return Err(e.to_string());
                }
            }
            
            // Send heartbeat to keep connection alive
            _ = heartbeat_timer.tick() => {
                let ping_msg = SyncMessage::Ping;
                if let Err(e) = send_message(&mut write_stream, &ping_msg).await {
                    eprintln!("❌ Failed to send heartbeat: {}", e);
                    return Err(e);
                }
            }
            
            // Handle incoming messages from server
            result = receive_message(&mut reader) => {
                match result {
                    Ok(Some(SyncMessage::ServerSync { edits, server_version })) => {
                        if !edits.is_empty() {
                            let mut engine = sync_engine.lock().await;
                            let old_content = engine.text().to_string();
                            
                            if let Err(e) = engine.apply_edits(edits.clone()) {
                                eprintln!("❌ Failed to apply server edits: {}", e);
                            } else {
                                let new_content = engine.text().to_string();
                                
                                // Show live collaboration feedback
                                println!("\n🌍 {} {} edits from other users! (v{})", 
                                    "LIVE UPDATE:".green().bold(),
                                    edits.len().to_string().cyan(),
                                    server_version.to_string().dimmed()
                                );
                                println!("📄 Document: \"{}\"", 
                                    truncate_text(&new_content, 70).blue()
                                );
                                
                                if old_content != new_content {
                                    println!("✨ {} → {}", 
                                        truncate_text(&old_content, 25).dimmed(),
                                        truncate_text(&new_content, 25).green()
                                    );
                                }
                                
                                print!("\n> ");
                                io::stdout().flush().unwrap();
                            }
                        }
                    }
                    Ok(Some(SyncMessage::Error { message })) => {
                        eprintln!("❌ Server error: {}", message.red());
                    }
                    Ok(Some(SyncMessage::Pong)) => {
                        // Heartbeat response - connection is healthy
                    }
                    Ok(Some(_)) => {
                        eprintln!("⚠️  Unexpected message from server");
                    }
                    Ok(None) => {
                        // Timeout occurred, but that's normal - continue waiting
                    }
                    Err(e) => {
                        eprintln!("❌ Connection error: {}", e);
                        return Err(e);
                    }
                }
            }
        }
    }
}

async fn interactive_command_loop(
    sync_engine: Arc<Mutex<SyncEngine>>,
    client_id: String,
) -> Result<(), String> {
    println!("\n{}", "🎮 Interactive Collaborative Editor".bold().cyan());
    println!("Commands:");
    println!("  {} <text>    - Replace document with new text", "edit".green());
    println!("  {}           - Show current document", "show".yellow());
    println!("  {}           - Show sync statistics", "stats".blue());
    println!("  {}           - Quit", "quit".red());
    println!("  {}           - Show help", "help".white());
    
    loop {
        print!("\n> ");
        io::stdout().flush().map_err(|e| e.to_string())?;
        
        let mut input = String::new();
        io::stdin().read_line(&mut input).map_err(|e| e.to_string())?;
        let input = input.trim();
        
        if input.is_empty() {
            continue;
        }
        
        let parts: Vec<&str> = input.splitn(2, ' ').collect();
        let command = parts[0];
        
        match command {
            "edit" => {
                if parts.len() > 1 {
                    let new_content = parts[1];
                    {
                        let mut engine = sync_engine.lock().await;
                        let old_content = engine.text().to_string();
                        engine.edit(new_content);
                        
                        if old_content != new_content {
                            println!("✏️  {} edited: \"{}\" → \"{}\"", 
                                client_id.green(),
                                truncate_text(&old_content, 30).dimmed(),
                                truncate_text(new_content, 30).blue()
                            );
                            println!("📤 {} (sync in progress...)", "Changes staged".yellow());
                        } else {
                            println!("💭 No changes made");
                        }
                    }
                } else {
                    println!("Usage: edit <text>");
                }
            }
            "show" => {
                let engine = sync_engine.lock().await;
                println!("📄 Current document: \"{}\"", engine.text().blue());
                println!("📊 Version: {}, Length: {} chars", 
                    engine.document().version.to_string().cyan(),
                    engine.text().len().to_string().yellow()
                );
            }
            "stats" => {
                let engine = sync_engine.lock().await;
                let stats = engine.stats();
                println!("📈 {} Statistics:", client_id.green().bold());
                println!("  Document version: {}", stats.document_version.to_string().cyan());
                println!("  Document length: {} chars", stats.document_length.to_string().yellow());
                println!("  Shadow checksum: {}", stats.shadow_checksum.dimmed());
                println!("  Has backup: {}", if stats.has_backup { "✅" } else { "❌" });
            }
            "help" => {
                println!("\n{}", "Available Commands:".bold());
                println!("  {} <text> - Replace document content", "edit".green());
                println!("  {}        - Show current document", "show".yellow());
                println!("  {}        - Show sync statistics", "stats".blue());
                println!("  {}        - Quit application", "quit".red());
                println!("  {}        - Show this help", "help".white());
            }
            "quit" | "exit" => {
                println!("👋 {} leaving the session...", client_id.green());
                break;
            }
            _ => {
                println!("❓ Unknown command: '{}'. Type '{}' for help.", 
                    command.red(), "help".white());
            }
        }
    }
    
    Ok(())
}

fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len.saturating_sub(3)])
    }
}

async fn send_message(
    stream: &mut tokio::net::tcp::OwnedWriteHalf,
    message: &SyncMessage,
) -> Result<(), String> {
    let data = serialize_message(message).map_err(|e| e.to_string())?;
    stream.write_all(&data).await.map_err(|e| e.to_string())?;
    Ok(())
}

async fn receive_message(
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
) -> Result<Option<SyncMessage>, String> {
    let mut line = String::new();
    
    // Use a longer timeout and make it non-fatal
    match timeout(Duration::from_secs(60), reader.read_line(&mut line)).await {
        Ok(Ok(0)) => {
            return Err("Connection closed".to_string());
        }
        Ok(Ok(_)) => {
            let message = deserialize_message(line.as_bytes())?;
            Ok(Some(message))
        }
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => {
            // Timeout is normal when no messages - return None instead of error
            Ok(None)
        },
    }
}

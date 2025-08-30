use clap::Parser;
use colored::*;
use diff_sync::{deserialize_message, serialize_message, SharedSyncServer, SyncMessage, SyncServer, DocumentDB};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};

#[derive(Parser)]
#[command(name = "sync-server")]
#[command(about = "Differential synchronization server with SQLite persistence")]
struct Cli {
    #[arg(short, long, default_value = "127.0.0.1:8080")]
    address: String,
    
    #[arg(short, long, default_value = "documents.db")]
    database_path: String,

    #[arg(short, long, default_value = "main")]
    document_name: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    println!("{}", "🚀 Starting Differential Sync Server with Persistence".green().bold());
    println!("📍 Address: {}", cli.address.cyan());
    println!("💾 Database: {}", cli.database_path.cyan());
    println!("📄 Document: {}", cli.document_name.cyan());
    
    // Create database connection
    let db = DocumentDB::new(&cli.database_path)
        .map_err(|e| format!("Failed to create database: {}", e))?;
    
    // Create shared server state with persistence
    let server = Arc::new(Mutex::new(
        SyncServer::new_with_db(db, cli.document_name.clone())
            .map_err(|e| format!("Failed to create server: {}", e))?
    ));

    // Show initial document from database
    {
        let server_lock = server.lock().await;
        let current_doc = server_lock.get_current_document()
            .map_err(|e| format!("Failed to load document: {}", e))?;
        println!("📄 Initial content: \"{}\"", current_doc.content.blue());
    }
    
    // Start TCP listener
    let listener = TcpListener::bind(&cli.address).await?;
    println!("✅ Server listening on {}", cli.address.green());
    
    // Start cleanup task for stale connections
    let cleanup_server = Arc::clone(&server);
    tokio::spawn(async move {
        let mut cleanup_timer = interval(Duration::from_secs(30));
        loop {
            cleanup_timer.tick().await;
            cleanup_server.lock().await.cleanup_stale_clients(120); // 2 minute timeout
        }
    });

    // Start status reporting task
    let status_server = Arc::clone(&server);
    tokio::spawn(async move {
        let mut status_timer = interval(Duration::from_secs(10));
        loop {
            status_timer.tick().await;
            let server_lock = status_server.lock().await;
            let clients = server_lock.get_connected_clients();
            if !clients.is_empty() {
                let doc_content = server_lock.get_document_content()
                    .unwrap_or_else(|_| "Error loading document".to_string());
                println!("📊 Active clients: {} | Document: \"{}\"", 
                    clients.len().to_string().cyan(),
                    truncate_text(&doc_content, 40).dimmed()
                );
            }
        }
    });
    
    // Accept connections
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                println!("🔌 New connection from {}", addr.to_string().yellow());
                let server_clone = Arc::clone(&server);
                tokio::spawn(async move {
                    if let Err(e) = handle_client(stream, server_clone).await {
                        eprintln!("❌ Client error: {}", e.to_string().red());
                    }
                });
            }
            Err(e) => {
                eprintln!("❌ Failed to accept connection: {}", e);
            }
        }
    }
}

async fn handle_client(
    stream: TcpStream,
    server: SharedSyncServer,
) -> Result<(), String> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    let mut client_id: Option<String> = None;
    
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                // Connection closed
                if let Some(id) = &client_id {
                    server.lock().await.disconnect_client(id);
                }
                break;
            }
            Ok(_) => {
                // Process message
                match deserialize_message(line.as_bytes()) {
                    Ok(message) => {
                        let response = handle_message(message, &server, &mut client_id).await;
                        
                        if let Some(response_msg) = response {
                            match serialize_message(&response_msg) {
                                Ok(data) => {
                                    if let Err(e) = write_half.write_all(&data).await {
                                        eprintln!("Failed to send response: {}", e);
                                        return Err(e.to_string());
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Failed to serialize response: {}", e);
                                }
                            }
                        }
                    }
                    Err(error_string) => {
                        eprintln!("Failed to parse message: {}", error_string);
                        let error_msg = SyncMessage::Error {
                            message: format!("Invalid message format: {}", error_string),
                        };
                        if let Ok(data) = serialize_message(&error_msg) {
                            let _ = write_half.write_all(&data).await;
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Connection error: {}", e);
                return Err(e.to_string());
            }
        }
    }
    
    Ok(())
}

async fn handle_message(
    message: SyncMessage,
    server: &SharedSyncServer,
    client_id: &mut Option<String>,
) -> Option<SyncMessage> {
    match message {
        SyncMessage::Connect { client_id: id } => {
            println!("🤝 Client {} requesting connection", id.green());
            let mut server_lock = server.lock().await;
            
            match server_lock.connect_client(id.clone()) {
                Ok(document) => {
                    *client_id = Some(id);
                    Some(SyncMessage::ConnectOk { 
                        server_version: server_lock.version,
                        document,
                    })
                }
                Err(error) => {
                    Some(SyncMessage::Error { message: error })
                }
            }
        }
        
        SyncMessage::ClientSync { client_id: id, edits, client_version: _ } => {
            // Handle both sending edits AND requesting updates
            let mut server_lock = server.lock().await;
            
            // Log activity
            if !edits.is_empty() {
                println!("🔄 Client {} syncing {} edits", id.cyan(), edits.len().to_string().yellow());
            }
            
            match server_lock.sync_with_client(&id, edits) {
                Ok(server_edits) => {
                    // Always respond with current state (even if no changes)
                    // This enables continuous sync for all clients
                    Some(SyncMessage::ServerSync {
                        edits: server_edits,
                        server_version: server_lock.version,
                    })
                }
                Err(error) => {
                    Some(SyncMessage::Error { message: error })
                }
            }
        }
        
        SyncMessage::Disconnect { client_id: id } => {
            server.lock().await.disconnect_client(&id);
            None
        }
        
        SyncMessage::Ping => {
            Some(SyncMessage::Pong)
        }
        
        _ => {
            Some(SyncMessage::Error {
                message: "Unexpected message type".to_string(),
            })
        }
    }
}

fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len.saturating_sub(3)])
    }
}

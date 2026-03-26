use clap::Parser;
use colored::*;
use diff_sync::{
    deserialize_message, handle_sync_message, serialize_message, truncate_text, DocumentDB,
    SharedSyncServer, SyncMessage, SyncServer,
};
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

    println!("{}", "Starting Sync Server".green().bold());
    println!("Address: {}", cli.address.cyan());
    println!("Database: {}", cli.database_path.cyan());

    let db = DocumentDB::new(&cli.database_path)
        .map_err(|e| format!("Failed to create database: {e}"))?;

    let server: SharedSyncServer = Arc::new(Mutex::new(
        SyncServer::new_with_db(db, cli.document_name)
            .map_err(|e| format!("Failed to create server: {e}"))?,
    ));

    {
        let lock = server.lock().await;
        let doc = lock
            .get_current_document()
            .map_err(|e| format!("Failed to load document: {e}"))?;
        println!("Initial content: \"{}\"", doc.content.blue());
    }

    let listener = TcpListener::bind(&cli.address).await?;
    println!("Listening on {}", cli.address.green());

    spawn_cleanup_task(Arc::clone(&server));
    spawn_status_task(Arc::clone(&server));

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                println!("New connection from {}", addr.to_string().yellow());
                let server_clone = Arc::clone(&server);
                tokio::spawn(async move {
                    if let Err(e) = handle_client(stream, server_clone).await {
                        eprintln!("Client error: {}", e.to_string().red());
                    }
                });
            }
            Err(e) => eprintln!("Failed to accept connection: {e}"),
        }
    }
}

fn spawn_cleanup_task(server: SharedSyncServer) {
    tokio::spawn(async move {
        let mut timer = interval(Duration::from_secs(30));
        loop {
            timer.tick().await;
            server.lock().await.cleanup_stale_clients(120);
        }
    });
}

fn spawn_status_task(server: SharedSyncServer) {
    tokio::spawn(async move {
        let mut timer = interval(Duration::from_secs(10));
        loop {
            timer.tick().await;
            let lock = server.lock().await;
            let clients = lock.get_connected_clients();
            if !clients.is_empty() {
                let content = lock
                    .get_document_content()
                    .unwrap_or_else(|_| "Error loading document".to_string());
                println!(
                    "Active clients: {} | Document: \"{}\"",
                    clients.len().to_string().cyan(),
                    truncate_text(&content, 40).dimmed()
                );
            }
        }
    });
}

async fn handle_client(stream: TcpStream, server: SharedSyncServer) -> Result<(), String> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    let mut client_id: Option<String> = None;

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                if let Some(id) = &client_id {
                    server.lock().await.disconnect_client(id);
                }
                break;
            }
            Ok(_) => match deserialize_message(line.as_bytes()) {
                Ok(message) => {
                    if let Some(response) =
                        handle_sync_message(message, &server, &mut client_id).await
                    {
                        let data = serialize_message(&response).map_err(|e| e.to_string())?;
                        write_half
                            .write_all(&data)
                            .await
                            .map_err(|e| e.to_string())?;
                    }
                }
                Err(e) => {
                    eprintln!("Failed to parse message: {e}");
                    let error_msg = SyncMessage::Error {
                        message: format!("Invalid message format: {e}"),
                    };
                    if let Ok(data) = serialize_message(&error_msg) {
                        let _ = write_half.write_all(&data).await;
                    }
                }
            },
            Err(e) => return Err(e.to_string()),
        }
    }

    Ok(())
}

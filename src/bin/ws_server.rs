use axum::{
    extract::ws::{Message, WebSocket},
    extract::{State, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use clap::Parser;
use colored::*;
use diff_sync::{
    handle_sync_message, truncate_text, DocumentDB, SharedSyncServer, SyncMessage, SyncServer,
};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

#[derive(Parser)]
#[command(name = "ws-server")]
#[command(about = "Differential synchronization server with TCP + WebSocket support")]
struct Cli {
    #[arg(long, default_value = "0.0.0.0:8080")]
    tcp_address: String,

    #[arg(long, default_value = "0.0.0.0:8081")]
    ws_address: String,

    #[arg(short, long, default_value = "documents.db")]
    database_path: String,

    #[arg(short, long, default_value = "main")]
    document_name: String,

    /// Path to the Next.js static export directory
    #[arg(long, default_value = "web/out")]
    static_dir: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    println!(
        "{}",
        "Starting Sync Server (TCP + WebSocket)".green().bold()
    );
    println!("TCP: {}", cli.tcp_address.cyan());
    println!("WS:  {}", cli.ws_address.cyan());
    println!("DB:  {}", cli.database_path.cyan());

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

    spawn_cleanup_task(Arc::clone(&server));
    spawn_status_task(Arc::clone(&server));

    // TCP listener in background
    let tcp_server = Arc::clone(&server);
    let tcp_addr = cli.tcp_address.clone();
    tokio::spawn(async move {
        if let Err(e) = run_tcp_listener(tcp_server, &tcp_addr).await {
            eprintln!("TCP listener error: {e}");
        }
    });

    // WebSocket + static file server (foreground with graceful shutdown)
    let index = ServeFile::new(format!("{}/index.html", &cli.static_dir));
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(Arc::clone(&server))
        .fallback_service(ServeDir::new(&cli.static_dir).not_found_service(index))
        .layer(CorsLayer::permissive());

    let ws_listener = tokio::net::TcpListener::bind(&cli.ws_address).await?;
    println!("TCP listening on {}", cli.tcp_address.green());
    println!("WS  listening on {}", cli.ws_address.green());

    axum::serve(ws_listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    println!("Server shutting down...");
    Ok(())
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

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl+C handler");
    println!("\nShutdown signal received");
}

// WebSocket Connections

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(server): State<SharedSyncServer>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_ws_client(socket, server))
}

async fn handle_ws_client(mut socket: WebSocket, server: SharedSyncServer) {
    let mut client_id: Option<String> = None;
    println!("New WebSocket connection");

    while let Some(Ok(msg)) = socket.recv().await {
        match msg {
            Message::Text(text) => {
                let parsed: Result<SyncMessage, _> = serde_json::from_str(&text);
                match parsed {
                    Ok(message) => {
                        let response = handle_sync_message(message, &server, &mut client_id).await;
                        if let Some(resp) = response {
                            match serde_json::to_string(&resp) {
                                Ok(json) => {
                                    if socket.send(Message::Text(json.into())).await.is_err() {
                                        break;
                                    }
                                }
                                Err(e) => eprintln!("Failed to serialize WS response: {e}"),
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to parse WS message: {e}");
                        let err = SyncMessage::Error {
                            message: format!("Invalid message format: {e}"),
                        };
                        if let Ok(json) = serde_json::to_string(&err) {
                            let _ = socket.send(Message::Text(json.into())).await;
                        }
                    }
                }
            }
            Message::Close(_) => break,
            _ => {} // axum handles ping/pong automatically
        }
    }

    if let Some(id) = &client_id {
        println!("WebSocket client {} disconnected", id);
        server.lock().await.disconnect_client(id);
    }
}

//  TCP Connections

async fn run_tcp_listener(
    server: SharedSyncServer,
    address: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(address).await?;

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                println!("New TCP connection from {}", addr.to_string().yellow());
                let server_clone = Arc::clone(&server);
                tokio::spawn(async move {
                    if let Err(e) = handle_tcp_client(stream, server_clone).await {
                        eprintln!("TCP client error: {}", e.to_string().red());
                    }
                });
            }
            Err(e) => eprintln!("Failed to accept TCP connection: {e}"),
        }
    }
}

async fn handle_tcp_client(stream: TcpStream, server: SharedSyncServer) -> Result<(), String> {
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
            Ok(_) => match diff_sync::deserialize_message(line.as_bytes()) {
                Ok(message) => {
                    if let Some(response) =
                        handle_sync_message(message, &server, &mut client_id).await
                    {
                        let data =
                            diff_sync::serialize_message(&response).map_err(|e| e.to_string())?;
                        write_half
                            .write_all(&data)
                            .await
                            .map_err(|e| e.to_string())?;
                    }
                }
                Err(e) => {
                    eprintln!("Failed to parse TCP message: {e}");
                    let err = SyncMessage::Error {
                        message: format!("Invalid message format: {e}"),
                    };
                    if let Ok(data) = diff_sync::serialize_message(&err) {
                        let _ = write_half.write_all(&data).await;
                    }
                }
            },
            Err(e) => return Err(e.to_string()),
        }
    }

    Ok(())
}

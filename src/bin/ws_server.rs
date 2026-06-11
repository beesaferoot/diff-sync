use axum::{
    extract::ws::{Message, WebSocket},
    extract::{Path, Query, State, WebSocketUpgrade},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use colored::*;
use diff_sync::{
    handle_sync_message, DocumentDB, SessionError, SessionManager,
    SharedSessionManager, SharedSyncServer, SyncMessage,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};
use tokio::time::{interval, Duration};
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

#[derive(Parser)]
#[command(name = "ws-server")]
#[command(about = "Differential synchronization server with TCP + WebSocket support")]
struct Cli {
    #[arg(long, default_value = "127.0.0.1:8080")]
    tcp_address: String,

    #[arg(long, default_value = "127.0.0.1:8081")]
    ws_address: String,

    #[arg(short, long, default_value = "documents.db")]
    database_path: String,

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

    DocumentDB::new(&cli.database_path)
        .map_err(|e| format!("Failed to initialize database: {e}"))?;

    let manager: SharedSessionManager = Arc::new(Mutex::new(SessionManager::new(
        cli.database_path.clone(),
    )));

    spawn_cleanup_task(Arc::clone(&manager));

    let tcp_manager = Arc::clone(&manager);
    let tcp_addr = cli.tcp_address.clone();
    tokio::spawn(async move {
        if let Err(e) = run_tcp_listener(tcp_manager, &tcp_addr).await {
            eprintln!("TCP listener error: {e}");
        }
    });

    let index = ServeFile::new(format!("{}/index.html", &cli.static_dir));
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/sessions", post(create_session_handler))
        .route("/api/sessions/:token", get(get_session_handler))
        .route("/api/sessions/:token/close", post(close_session_handler))
        .route("/health", get(|| async { "ok" }))
        .with_state(Arc::clone(&manager))
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

fn spawn_cleanup_task(manager: SharedSessionManager) {
    tokio::spawn(async move {
        let mut timer = interval(Duration::from_secs(30));
        loop {
            timer.tick().await;
            let mut mgr = manager.lock().await;
            mgr.cleanup_stale_clients(120).await;
            mgr.cleanup_idle_sessions(Duration::from_secs(300)).await;
        }
    });
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl+C handler");
    println!("\nShutdown signal received");
}

#[derive(Deserialize)]
struct CreateSessionRequest {
    #[serde(default)]
    initial_content: Option<String>,
}

#[derive(Serialize)]
struct CreateSessionResponse {
    token: String,
    creator_secret: String,
    url: String,
}

async fn create_session_handler(
    State(manager): State<SharedSessionManager>,
    Json(body): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    let content = body.initial_content.as_deref().unwrap_or("");
    let mgr = manager.lock().await;
    match mgr.create_session(content) {
        Ok((token, creator_secret)) => {
            let url = format!("/s/{token}");
            println!("Session created: {}", token.green());
            (
                StatusCode::CREATED,
                Json(CreateSessionResponse {
                    token,
                    creator_secret,
                    url,
                }),
            )
                .into_response()
        }
        Err(e) => {
            eprintln!("Failed to create session: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e).into_response()
        }
    }
}

#[derive(Serialize)]
struct SessionInfoResponse {
    token: String,
    status: String,
    created_at: i64,
    closed_at: Option<i64>,
}

async fn get_session_handler(
    State(manager): State<SharedSessionManager>,
    Path(token): Path<String>,
) -> impl IntoResponse {
    let mgr = manager.lock().await;
    match mgr.get_session(&token) {
        Ok(session) => (
            StatusCode::OK,
            Json(SessionInfoResponse {
                token: session.token,
                status: session.status,
                created_at: session.created_at,
                closed_at: session.closed_at,
            }),
        )
            .into_response(),
        Err(SessionError::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct CloseSessionRequest {
    creator_secret: String,
}

#[derive(Serialize)]
struct CloseSessionResponse {
    ok: bool,
}

async fn close_session_handler(
    State(manager): State<SharedSessionManager>,
    Path(token): Path<String>,
    Json(body): Json<CloseSessionRequest>,
) -> impl IntoResponse {
    let mut mgr = manager.lock().await;
    match mgr.close_session(&token, &body.creator_secret).await {
        Ok(()) => {
            println!("Session closed: {}", token.yellow());
            (StatusCode::OK, Json(CloseSessionResponse { ok: true })).into_response()
        }
        Err(SessionError::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Err(SessionError::Closed) => {
            (StatusCode::GONE, "Session already closed").into_response()
        }
        Err(SessionError::Forbidden) => {
            (StatusCode::FORBIDDEN, "Invalid creator secret").into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct WsParams {
    session: Option<String>,
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WsParams>,
    State(manager): State<SharedSessionManager>,
) -> impl IntoResponse {
    let session_token = params.session;

    if let Some(ref token) = session_token {
        let mgr = manager.lock().await;
        match mgr.get_session(token) {
            Ok(session) if session.status == "active" => {}
            Ok(_) => {
                return (StatusCode::GONE, "Session has ended").into_response();
            }
            Err(SessionError::NotFound) => {
                return (StatusCode::NOT_FOUND, "Session not found").into_response();
            }
            Err(e) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
        }
    }

    ws.on_upgrade(move |socket| handle_ws_client(socket, manager, session_token))
        .into_response()
}

/// Resolves when the session closes. Never resolves for the default server
/// (no signal), so `select!` falls through to the socket.
async fn await_session_closed(shutdown: &mut Option<broadcast::Receiver<()>>) {
    match shutdown {
        Some(rx) => {
            let _ = rx.recv().await;
        }
        None => std::future::pending::<()>().await,
    }
}

async fn handle_ws_client(
    mut socket: WebSocket,
    manager: SharedSessionManager,
    session_token: Option<String>,
) {
    let (server, mut shutdown) = {
        let mut mgr = manager.lock().await;
        if let Some(ref token) = session_token {
            match mgr.get_or_start_session(token) {
                Ok((s, rx)) => (s, Some(rx)),
                Err(SessionError::Closed) => {
                    let closed = SyncMessage::SessionClosed;
                    if let Ok(json) = serde_json::to_string(&closed) {
                        let _ = socket.send(Message::Text(json.into())).await;
                    }
                    return;
                }
                Err(e) => {
                    eprintln!("Failed to start session: {e}");
                    let err = SyncMessage::Error {
                        message: e.to_string(),
                    };
                    if let Ok(json) = serde_json::to_string(&err) {
                        let _ = socket.send(Message::Text(json.into())).await;
                    }
                    return;
                }
            }
        } else {
            match mgr.default_server() {
                Ok(s) => (s, None),
                Err(e) => {
                    eprintln!("Failed to start default server: {e}");
                    return;
                }
            }
        }
    };

    let label = session_token
        .as_deref()
        .map(|t| format!("session {}", &t[..t.len().min(8)]))
        .unwrap_or_else(|| "default".to_string());
    println!("New WebSocket connection ({})", label.cyan());

    let mut client_id: Option<String> = None;

    loop {
        tokio::select! {
            _ = await_session_closed(&mut shutdown) => {
                println!("Session closed, disconnecting client ({})", label.yellow());
                let closed = SyncMessage::SessionClosed;
                if let Ok(json) = serde_json::to_string(&closed) {
                    let _ = socket.send(Message::Text(json.into())).await;
                }
                let _ = socket.send(Message::Close(None)).await;
                break;
            }
            incoming = socket.recv() => {
                let msg = match incoming {
                    Some(Ok(msg)) => msg,
                    _ => break,
                };
                match msg {
                    Message::Text(text) => {
                        let parsed: Result<SyncMessage, _> = serde_json::from_str(&text);
                        match parsed {
                            Ok(message) => {
                                let response =
                                    handle_sync_message(message, &server, &mut client_id).await;
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
                    _ => {}
                }
            }
        }
    }

    if let Some(id) = &client_id {
        println!("WebSocket client {} disconnected ({})", id, label);
        server.lock().await.disconnect_client(id);
    }
}

async fn run_tcp_listener(
    manager: SharedSessionManager,
    address: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(address).await?;

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                println!("New TCP connection from {}", addr.to_string().yellow());
                let mgr = Arc::clone(&manager);
                tokio::spawn(async move {
                    let server = {
                        let mut m = mgr.lock().await;
                        match m.default_server() {
                            Ok(s) => s,
                            Err(e) => {
                                eprintln!("Failed to start default server: {e}");
                                return;
                            }
                        }
                    };
                    if let Err(e) = handle_tcp_client(stream, server).await {
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

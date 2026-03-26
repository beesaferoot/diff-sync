use clap::Parser;
use colored::*;
use diff_sync::{deserialize_message, serialize_message, truncate_text, SyncEngine, SyncMessage};
use std::io::{self, Write};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::{interval, timeout, Duration};

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

    let client_id = cli
        .client_id
        .unwrap_or_else(|| format!("client_{}", rand::random::<u32>()));

    println!("{}", "Starting Sync Client".blue().bold());
    println!("Server: {}", cli.server.cyan());
    println!("Client ID: {}", client_id.green());

    let stream = TcpStream::connect(&cli.server).await?;
    println!("Connected to server");

    let (read_half, write_half) = stream.into_split();

    let connect_msg = SyncMessage::Connect {
        client_id: client_id.clone(),
    };
    let mut write_stream = write_half;
    send_message(&mut write_stream, &connect_msg).await?;

    let mut reader = BufReader::new(read_half);
    let sync_engine = match receive_message(&mut reader).await? {
        Some(SyncMessage::ConnectOk {
            server_version,
            document,
        }) => {
            println!(
                "Connected to server (v{})",
                server_version.to_string().cyan()
            );
            println!("Initial document: \"{}\"", document.content.blue());

            let mut engine = SyncEngine::new(document.content);
            engine.node_id = client_id.clone();
            Arc::new(Mutex::new(engine))
        }
        Some(SyncMessage::Error { message }) => {
            eprintln!("Connection failed: {}", message.red());
            return Ok(());
        }
        Some(_) => {
            eprintln!("Unexpected response from server");
            return Ok(());
        }
        None => {
            eprintln!("Connection timeout during handshake");
            return Ok(());
        }
    };

    let sync_engine_bg = Arc::clone(&sync_engine);
    let client_id_bg = client_id.clone();
    let sync_task = tokio::spawn(async move {
        if let Err(e) = background_sync(write_stream, reader, sync_engine_bg, client_id_bg).await {
            eprintln!("Sync task error: {e}");
        }
    });

    let sync_engine_ui = Arc::clone(&sync_engine);
    let interactive_task =
        tokio::spawn(async move { interactive_loop(sync_engine_ui, client_id).await });

    tokio::select! {
        _ = sync_task => println!("Sync task ended"),
        _ = interactive_task => println!("Interactive session ended"),
    }

    Ok(())
}

async fn background_sync(
    mut writer: tokio::net::tcp::OwnedWriteHalf,
    mut reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    engine: Arc<Mutex<SyncEngine>>,
    client_id: String,
) -> Result<(), String> {
    let mut sync_timer = interval(Duration::from_millis(500));
    let mut heartbeat_timer = interval(Duration::from_secs(30));

    loop {
        tokio::select! {
            _ = sync_timer.tick() => {
                let (edits, version) = {
                    let mut eng = engine.lock().await;
                    (eng.diff_and_update_shadow(), eng.document().version)
                };

                let msg = SyncMessage::ClientSync {
                    client_id: client_id.clone(),
                    edits,
                    client_version: version,
                    cursor_position: None,
                };

                send_message(&mut writer, &msg).await.map_err(|e| e.to_string())?;
            }

            _ = heartbeat_timer.tick() => {
                send_message(&mut writer, &SyncMessage::Ping).await?;
            }

            result = receive_message(&mut reader) => {
                match result {
                    Ok(Some(SyncMessage::ServerSync { edits, server_version, .. })) => {
                        if !edits.is_empty() {
                            let mut eng = engine.lock().await;
                            let old = eng.text().to_string();

                            if let Err(e) = eng.apply_edits(edits.clone()) {
                                eprintln!("Failed to apply server edits: {e}");
                            } else {
                                let new = eng.text().to_string();
                                println!(
                                    "\n{} {} edits (v{})",
                                    "LIVE UPDATE:".green().bold(),
                                    edits.len().to_string().cyan(),
                                    server_version.to_string().dimmed()
                                );
                                println!("Document: \"{}\"", truncate_text(&new, 70).blue());

                                if old != new {
                                    println!(
                                        "{} -> {}",
                                        truncate_text(&old, 25).dimmed(),
                                        truncate_text(&new, 25).green()
                                    );
                                }

                                print!("\n> ");
                                io::stdout().flush().unwrap();
                            }
                        }
                    }
                    Ok(Some(SyncMessage::Error { message })) => {
                        eprintln!("Server error: {}", message.red());
                    }
                    Ok(Some(SyncMessage::Pong)) => {}
                    Ok(Some(_)) => {
                        eprintln!("Unexpected message from server");
                    }
                    Ok(None) => {}
                    Err(e) => return Err(e),
                }
            }
        }
    }
}

async fn interactive_loop(engine: Arc<Mutex<SyncEngine>>, client_id: String) -> Result<(), String> {
    println!("\n{}", "Interactive Collaborative Editor".bold().cyan());
    println!("Commands: edit <text>, show, stats, help, quit\n");

    loop {
        print!("> ");
        io::stdout().flush().map_err(|e| e.to_string())?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| e.to_string())?;
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        let (cmd, arg) = input
            .split_once(' ')
            .map(|(c, a)| (c, Some(a)))
            .unwrap_or((input, None));

        match cmd {
            "edit" => {
                if let Some(text) = arg {
                    let mut eng = engine.lock().await;
                    let old = eng.text().to_string();
                    eng.edit(text);

                    if old != text {
                        println!(
                            "{} edited: \"{}\" -> \"{}\"",
                            client_id.green(),
                            truncate_text(&old, 30).dimmed(),
                            truncate_text(text, 30).blue()
                        );
                    } else {
                        println!("No changes");
                    }
                } else {
                    println!("Usage: edit <text>");
                }
            }
            "show" => {
                let eng = engine.lock().await;
                println!("Document: \"{}\"", eng.text().blue());
                println!(
                    "Version: {}, Length: {} chars",
                    eng.document().version.to_string().cyan(),
                    eng.text().len().to_string().yellow()
                );
            }
            "stats" => {
                let eng = engine.lock().await;
                let stats = eng.stats();
                println!("{} Statistics:", client_id.green().bold());
                println!("  Version: {}", stats.document_version.to_string().cyan());
                println!(
                    "  Length: {} chars",
                    stats.document_length.to_string().yellow()
                );
                println!("  Shadow checksum: {}", stats.shadow_checksum.dimmed());
                println!("  Has backup: {}", stats.has_backup);
            }
            "help" => {
                println!("\n{}", "Commands:".bold());
                println!("  {} <text> - Replace document", "edit".green());
                println!("  {}        - Show document", "show".yellow());
                println!("  {}        - Show statistics", "stats".blue());
                println!("  {}        - Quit", "quit".red());
            }
            "quit" | "exit" => {
                println!("{} leaving...", client_id.green());
                break;
            }
            _ => println!("Unknown command: '{}'. Type 'help'.", cmd.red()),
        }
    }

    Ok(())
}

async fn send_message(
    stream: &mut tokio::net::tcp::OwnedWriteHalf,
    message: &SyncMessage,
) -> Result<(), String> {
    let data = serialize_message(message).map_err(|e| e.to_string())?;
    stream.write_all(&data).await.map_err(|e| e.to_string())
}

async fn receive_message(
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
) -> Result<Option<SyncMessage>, String> {
    let mut line = String::new();
    match timeout(Duration::from_secs(60), reader.read_line(&mut line)).await {
        Ok(Ok(0)) => Err("Connection closed".to_string()),
        Ok(Ok(_)) => {
            let msg = deserialize_message(line.as_bytes())?;
            Ok(Some(msg))
        }
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Ok(None), // Timeout — normal when idle
    }
}

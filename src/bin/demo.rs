use clap::{Parser, Subcommand};
use colored::*;
use diff_sync::{truncate_text, SyncEngine, SyncResult};
use std::io::{self, Write};

#[derive(Parser)]
#[command(name = "sync-demo")]
#[command(about = "Interactive demonstration of differential synchronization")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Two-user editing simulation with manual sync
    Interactive {
        #[arg(
            short,
            long,
            default_value = "The quick brown fox jumps over the lazy dog"
        )]
        initial_text: String,
    },
    /// Automated concurrent edit simulation
    Simulate {
        #[arg(short, long, default_value = "10")]
        iterations: usize,
    },
    /// Throughput benchmark
    Benchmark {
        #[arg(short, long, default_value = "1000")]
        iterations: usize,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Interactive { initial_text } => run_interactive(initial_text),
        Commands::Simulate { iterations } => run_simulation(iterations),
        Commands::Benchmark { iterations } => run_benchmark(iterations),
    }
}

fn run_interactive(initial_text: String) {
    println!(
        "{}",
        "=== Differential Synchronization Demo ===".bold().cyan()
    );
    println!("Simulates two users editing the same document.");
    println!("Commands: 'a <text>' (Alice), 'b <text>' (Bob), 's' (sync), 'q' (quit)\n");

    let mut alice = SyncEngine::new(initial_text.clone());
    let mut bob = SyncEngine::new(initial_text);
    alice.node_id = "Alice".to_string();
    bob.node_id = "Bob".to_string();

    print_state(&alice, &bob);

    loop {
        print!("\n> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        match input.as_bytes()[0] {
            b'q' => {
                println!("Goodbye!");
                break;
            }
            b's' => {
                println!("{}", "=== Synchronizing ===".yellow());
                let (server_result, client_result) = alice.sync_with(&mut bob);
                print_sync_results(&server_result, &client_result);
                print_state(&alice, &bob);
            }
            b'a' if input.len() > 2 => {
                alice.edit(&input[2..]);
                println!("Alice edited document");
                print_state(&alice, &bob);
            }
            b'b' if input.len() > 2 => {
                bob.edit(&input[2..]);
                println!("Bob edited document");
                print_state(&alice, &bob);
            }
            b'h' | b'?' => print_help(),
            _ => println!("Unknown command. Type 'h' for help."),
        }
    }
}

fn run_simulation(iterations: usize) {
    println!("{}", "=== Concurrent Edit Simulation ===".bold().cyan());

    let mut alice = SyncEngine::new("The cat sat on the mat.".to_string());
    let mut bob = SyncEngine::new("The cat sat on the mat.".to_string());
    alice.node_id = "Alice".to_string();
    bob.node_id = "Bob".to_string();

    let alice_edits = [
        "The big cat sat on the mat.",
        "The big black cat sat on the mat.",
        "The big black cat sat on the soft mat.",
        "The big black cat sat comfortably on the soft mat.",
    ];

    let bob_edits = [
        "The cat sat on the red mat.",
        "The cat sat peacefully on the red mat.",
        "The cat sat peacefully on the red woolen mat.",
        "The friendly cat sat peacefully on the red woolen mat.",
    ];

    println!("Initial state:");
    print_state(&alice, &bob);

    let rounds = iterations.min(alice_edits.len()).min(bob_edits.len());
    for i in 0..rounds {
        println!("\n{}", format!("=== Iteration {} ===", i + 1).yellow());

        alice.edit(alice_edits[i]);
        bob.edit(bob_edits[i]);

        println!("After concurrent edits:");
        print_state(&alice, &bob);

        let (server_result, client_result) = alice.sync_with(&mut bob);

        println!("\nAfter synchronization:");
        print_sync_results(&server_result, &client_result);
        print_state(&alice, &bob);

        if alice.text() == bob.text() {
            println!("{}", "Documents are synchronized!".green());
        } else {
            println!("{}", "Documents are out of sync!".red());
        }
    }
}

fn run_benchmark(iterations: usize) {
    println!("{}", "=== Synchronization Benchmark ===".bold().cyan());

    let start = std::time::Instant::now();
    let mut successful_syncs = 0u64;
    let mut total_edits = 0u64;

    for i in 0..iterations {
        let mut alice = SyncEngine::new(format!("Document {i} content"));
        let mut bob = SyncEngine::new(format!("Document {i} content"));

        alice.edit(&format!("Alice modified document {i} with some changes"));
        bob.edit(&format!("Bob also modified document {i} differently"));

        let (a, b) = alice.sync_with(&mut bob);
        if a.success && b.success {
            successful_syncs += 1;
        }
        total_edits += (a.edits.len() + b.edits.len()) as u64;
    }

    let duration = start.elapsed();
    let pct = (successful_syncs as f64 / iterations as f64) * 100.0;

    println!("Completed {iterations} sync cycles in {duration:?}");
    println!("Successful: {successful_syncs} ({pct:.1}%)");
    println!("Total edits: {total_edits}");
    println!("Avg per sync: {:?}", duration / iterations as u32);
    println!(
        "Syncs/sec: {:.1}",
        iterations as f64 / duration.as_secs_f64()
    );
}

fn print_state(alice: &SyncEngine, bob: &SyncEngine) {
    println!("\n{}", "Current State:".bold());
    println!(
        "  {}: \"{}\"",
        "Alice".blue().bold(),
        truncate_text(alice.text(), 60)
    );
    println!(
        "  {}:   \"{}\"",
        "Bob".green().bold(),
        truncate_text(bob.text(), 60)
    );

    if alice.text() == bob.text() {
        println!("  {}", "Documents are in sync".green());
    } else {
        println!("  {}", "Documents differ".red());
    }
}

fn print_sync_results(server_result: &SyncResult, client_result: &SyncResult) {
    if !server_result.edits.is_empty() {
        println!(
            "  Alice -> Bob: {} edits",
            server_result.edits.len().to_string().cyan()
        );
    }
    if !client_result.edits.is_empty() {
        println!(
            "  Bob -> Alice: {} edits",
            client_result.edits.len().to_string().cyan()
        );
    }
    if server_result.edits.is_empty() && client_result.edits.is_empty() {
        println!("  {}", "No changes to sync".dimmed());
    }
}

fn print_help() {
    println!("\n{}", "Available Commands:".bold());
    println!("  {} <text>  - Edit Alice's document", "a".blue().bold());
    println!("  {} <text>  - Edit Bob's document", "b".green().bold());
    println!("  {}         - Synchronize documents", "s".yellow().bold());
    println!("  {}         - Show this help", "h".white().bold());
    println!("  {}         - Quit", "q".red().bold());
}

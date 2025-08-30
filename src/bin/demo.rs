use clap::{Parser, Subcommand};
use colored::*;
use diff_sync::{SyncEngine, SyncResult};
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
    /// Interactive demo with two users editing simultaneously
    Interactive {
        #[arg(short, long, default_value = "The quick brown fox jumps over the lazy dog")]
        initial_text: String,
    },
    /// Simulation of concurrent edits
    Simulate {
        #[arg(short, long, default_value = "10")]
        iterations: usize,
    },
    /// Benchmark synchronization performance
    Benchmark {
        #[arg(short, long, default_value = "1000")]
        iterations: usize,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Interactive { initial_text } => run_interactive_demo(initial_text),
        Commands::Simulate { iterations } => run_simulation(iterations),
        Commands::Benchmark { iterations } => run_benchmark(iterations),
    }
}

fn run_interactive_demo(initial_text: String) {
    println!("{}", "=== Differential Synchronization Demo ===".bold().cyan());
    println!("This demo simulates two users editing the same document.");
    println!("You can edit both 'Alice' and 'Bob' documents and see them sync.");
    println!("Commands: 'a <text>' (edit Alice), 'b <text>' (edit Bob), 's' (sync), 'q' (quit)\n");

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

        match input.chars().next() {
            Some('q') => {
                println!("Goodbye!");
                break;
            }
            Some('s') => {
                println!("{}", "=== Synchronizing ===".yellow());
                let (alice_result, bob_result) = alice.sync_with(&mut bob);
                print_sync_results(&alice_result, &bob_result);
                print_state(&alice, &bob);
            }
            Some('a') => {
                let text = input[1..].trim();
                if !text.is_empty() {
                    alice.edit(text);
                    println!("{} Alice edited document", "✏️".green());
                    print_state(&alice, &bob);
                }
            }
            Some('b') => {
                let text = input[1..].trim();
                if !text.is_empty() {
                    bob.edit(text);
                    println!("{} Bob edited document", "✏️".green());
                    print_state(&alice, &bob);
                }
            }
            Some('h') | Some('?') => {
                print_help();
            }
            _ => {
                println!("Unknown command. Type 'h' for help.");
            }
        }
    }
}

fn run_simulation(iterations: usize) {
    println!("{}", "=== Concurrent Edit Simulation ===".bold().cyan());
    
    let mut alice = SyncEngine::new("The cat sat on the mat.".to_string());
    let mut bob = SyncEngine::new("The cat sat on the mat.".to_string());
    alice.node_id = "Alice".to_string();
    bob.node_id = "Bob".to_string();

    let alice_edits = vec![
        "The big cat sat on the mat.",
        "The big black cat sat on the mat.",
        "The big black cat sat on the soft mat.",
        "The big black cat sat comfortably on the soft mat.",
    ];

    let bob_edits = vec![
        "The cat sat on the red mat.",
        "The cat sat peacefully on the red mat.",
        "The cat sat peacefully on the red woolen mat.",
        "The friendly cat sat peacefully on the red woolen mat.",
    ];

    println!("Initial state:");
    print_state(&alice, &bob);

    for i in 0..iterations.min(alice_edits.len()).min(bob_edits.len()) {
        println!("\n{}", format!("=== Iteration {} ===", i + 1).yellow());
        
        // Both users make edits
        alice.edit(alice_edits[i]);
        bob.edit(bob_edits[i]);
        
        println!("After concurrent edits:");
        print_state(&alice, &bob);
        
        // Synchronize
        let (alice_result, bob_result) = alice.sync_with(&mut bob);
        
        println!("\nAfter synchronization:");
        print_sync_results(&alice_result, &bob_result);
        print_state(&alice, &bob);
        
        // Verify they're in sync
        if alice.text() == bob.text() {
            println!("{} Documents are synchronized!", "✅".green());
        } else {
            println!("{} Documents are out of sync!", "❌".red());
        }
    }
}

fn run_benchmark(iterations: usize) {
    println!("{}", "=== Synchronization Benchmark ===".bold().cyan());
    
    let start = std::time::Instant::now();
    let mut successful_syncs = 0;
    let mut total_edits = 0;

    for i in 0..iterations {
        let mut alice = SyncEngine::new(format!("Document {} content", i));
        let mut bob = SyncEngine::new(format!("Document {} content", i));
        
        // Make some edits
        alice.edit(&format!("Alice modified document {} with some changes", i));
        bob.edit(&format!("Bob also modified document {} differently", i));
        
        // Sync
        let (alice_result, bob_result) = alice.sync_with(&mut bob);
        
        if alice_result.success && bob_result.success {
            successful_syncs += 1;
        }
        
        total_edits += alice_result.edits.len() + bob_result.edits.len();
    }

    let duration = start.elapsed();
    
    println!("Completed {} synchronization cycles in {:?}", iterations, duration);
    println!("Successful syncs: {} ({:.1}%)", successful_syncs, 
             (successful_syncs as f64 / iterations as f64) * 100.0);
    println!("Total edits processed: {}", total_edits);
    println!("Average time per sync: {:?}", duration / iterations as u32);
    println!("Syncs per second: {:.1}", iterations as f64 / duration.as_secs_f64());
}

fn print_state(alice: &SyncEngine, bob: &SyncEngine) {
    println!("\n{}", "Current State:".bold());
    println!("  {}: \"{}\"", 
             "Alice".blue().bold(), 
             truncate_text(alice.text(), 60));
    println!("  {}:   \"{}\"", 
             "Bob".green().bold(), 
             truncate_text(bob.text(), 60));
    
    if alice.text() == bob.text() {
        println!("  {}", "✅ Documents are in sync".green());
    } else {
        println!("  {}", "❌ Documents differ".red());
    }
}

fn print_sync_results(alice_result: &SyncResult, bob_result: &SyncResult) {
    if !alice_result.edits.is_empty() {
        println!("  Alice -> Bob: {}", format_edit_summary(&alice_result.edits));
    }
    if !bob_result.edits.is_empty() {
        println!("  Bob -> Alice: {}", format_edit_summary(&bob_result.edits));
    }
    
    if alice_result.edits.is_empty() && bob_result.edits.is_empty() {
        println!("  {}", "No changes to sync".dimmed());
    }
}

fn format_edit_summary(edits: &diff_sync::EditList) -> String {
    if edits.is_empty() {
        "no changes".dimmed().to_string()
    } else {
        format!("{} edits", edits.len()).cyan().to_string()
    }
}

fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len.saturating_sub(3)])
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

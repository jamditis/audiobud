// Import necessary modules
use std::fs;
use std::io;
use std::path::Path;

// Define a function to get the history manager
fn get_history_manager() -> History {
    let mut manager = History::new();
    manager.load().unwrap();
    manager
}

// Define a function to handle transcript history
fn handle_transcript_history(transcript: &str) {
    let mut history_manager = get_history_manager();
    let words: Vec<&str> = transcript.split_whitespace().collect();
    for word in words {
        history_manager.add_entry(word.to_string(), "".to_string());
    }
}

// Define a function to run the history command
fn run() -> io::Result<()> {
    // Load transcript history
    let transcript_history = fs::read_to_string("transcript_history.txt")?;
    handle_transcript_history(&transcript_history);

    Ok(())
}

fn main() {
    match run() {
        Ok(_) => println!("History command ran successfully"),
        Err(e) => println!("Error: {}", e),
    }
}
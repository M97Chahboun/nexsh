use chrono::{DateTime, Utc};
use colored::*;
use std::env;

pub fn print_header() {
    // ASCII Art Logo
    let logo = r#"
    ███╗   ██╗███████╗██╗  ██╗███████╗██╗  ██╗
    ████╗  ██║██╔════╝╚██╗██╔╝██╔════╝██║  ██║
    ██╔██╗ ██║█████╗   ╚███╔╝ ███████╗███████║
    ██║╚██╗██║██╔══╝   ██╔██╗ ╚════██║██╔══██║
    ██║ ╚████║███████╗██╔╝ ██╗███████║██║  ██║
    ╚═╝  ╚═══╝╚══════╝╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝"#;

    // System Info
    let username = env::var("USER").unwrap_or_else(|_| "unknown".to_string());
    let now: DateTime<Utc> = Utc::now();
    let version = env!("CARGO_PKG_VERSION");

    // Print Header
    println!("{}", logo.bright_cyan());
    println!("{}", "━".repeat(56).bright_blue());
    println!(
        "{} {} {} {} {} {} {}",
        "🤖".cyan(),
        "AI-Powered Shell".bright_white(),
        "|".bright_blue(),
        format!("v{}", version).yellow(),
        "|".bright_blue(),
        username.green(),
        now.format("(%Y-%m-%d %H:%M UTC)").to_string().bright_black()
    );
    println!("{}", "━".repeat(56).bright_blue());
    println!("🤖 NexSh Help:");
    println!("  - Type 'exit' or 'quit' to exit the shell.");
    println!("  - Type any command to execute it.");
    println!("  - Use 'init' to set up your API key.");
    println!("  - Use 'clear' to clear conversation context.");

    println!("\n{} Type {} for help or {} to exit", "→".bright_yellow(), "'help'".cyan(), "'exit'".cyan());
}
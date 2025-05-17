use clap::Parser;
use nexsh::Shell;
use std::error::Error;
mod header;
pub mod prompt;
pub mod types;

#[derive(Parser, Debug)]
#[command(
    name = "nexsh",
    version = "0.2.0",
    about = "AI-powered smart shell using Google Gemini"
)]
struct Args {
    /// Execute single command and exit
    #[arg(short, long)]
    execute: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    header::print_header();

    let args = Args::parse();
    let mut shell = Shell::new()?;

    if let Some(cmd) = args.execute {
        if cmd == "--help" || cmd == "-h" {
            shell.print_help()?;
            return Ok(());
        }
        return shell.process_command(&cmd).await;
    }

    shell.run().await
}

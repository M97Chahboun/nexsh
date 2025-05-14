use clap::Parser;
use nexsh::Shell;
use std::{
    error::Error,
};
pub mod types;
pub mod prompt;

#[derive(Parser, Debug)]
#[command(
    name = "nexsh",
    version = "0.1.0",
    about = "AI-powered smart shell using Google Gemini"
)]
struct Args {
    /// Initialize configuration
    #[arg(short, long)]
    init: bool,

    /// Execute single command and exit
    #[arg(short, long)]
    execute: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let mut shell = Shell::new()?;

    if args.init {
        shell.initialize()?;
        return Ok(());
    }

    if let Some(cmd) = args.execute {
        return shell.process_command(&cmd).await;
    }

    shell.run().await
}

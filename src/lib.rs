use clap::Parser;
use colored::*;
use directories::ProjectDirs;
use gemini_client_rs::{
    types::{GenerateContentRequest, PartResponse},
    GeminiClient,
};
use prompt::SYSTEM_PROMPT;
use rustyline::{error::ReadlineError, DefaultEditor};
use serde_json::json;
use std::{
    error::Error,
    fs,
    io::{self, Write},
    path::PathBuf,
    process::Command,
};
use types::{GeminiResponse, NexShConfig};
pub mod prompt;
pub mod types;

#[derive(Parser, Debug)]
#[command(
    name = "nexsh",
    version = "0.1.0",
    about = "Next-generation AI-powered shell using Google Gemini"
)]
struct Args {
    /// Initialize configuration
    #[arg(short, long)]
    init: bool,

    /// Execute single command and exit
    #[arg(short, long)]
    execute: Option<String>,
}

impl Default for NexShConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            history_size: 1000,
            default_os: std::env::consts::OS.to_string(),
        }
    }
}

pub struct Shell {
    config: NexShConfig,
    config_dir: PathBuf,
    history_file: PathBuf,
    client: GeminiClient,
    editor: DefaultEditor,
}

impl Shell {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let proj_dirs = ProjectDirs::from("com", "gemini-shell", "nexsh")
            .ok_or("Failed to get project directories")?;

        let config_dir = proj_dirs.config_dir().to_path_buf();
        fs::create_dir_all(&config_dir)?;

        let config_file = config_dir.join("nexsh_config.json");
        let history_file = config_dir.join("nexsh_history.txt");

        let config = if config_file.exists() {
            let content = fs::read_to_string(&config_file)?;
            serde_json::from_str(&content)?
        } else {
            NexShConfig::default()
        };

        let mut editor = DefaultEditor::new()?;
        if history_file.exists() {
            let _ = editor.load_history(&history_file);
        }

        let client = GeminiClient::new(config.api_key.clone());

        Ok(Self {
            config,
            config_dir,
            history_file,
            client,
            editor,
        })
    }

    fn save_config(&self) -> Result<(), Box<dyn Error>> {
        let config_file = self.config_dir.join("nexsh_config.json");
        let content = serde_json::to_string_pretty(&self.config)?;
        fs::write(config_file, content)?;
        Ok(())
    }

    pub fn initialize(&mut self) -> Result<(), Box<dyn Error>> {
        println!("🤖 Welcome to NexSh Setup!");

        print!("Enter your Gemini API key: ");
        io::stdout().flush()?;
        self.config.api_key = self
            .editor
            .readline("Enter your Gemini API key: ")?
            .trim()
            .to_string();

        self.save_config()?;
        println!("✅ Configuration saved successfully!");

        Ok(())
    }

    pub async fn process_command(&mut self, input: &str) -> Result<(), Box<dyn Error>> {
        let req_json = json!({
            "contents": [{
                "parts": [{
                    "text": SYSTEM_PROMPT
                        .replace("{OS}", &self.config.default_os)
                        .replace("{REQUEST}", input)
                }],
                "role": "user"
            }],
            "tools": []
        });

        let request: GenerateContentRequest = serde_json::from_value(req_json)?;
        let response = self
            .client
            .generate_content("gemini-1.5-flash", &request)
            .await?;

        if let Some(candidates) = response.candidates {
            for candidate in &candidates {
                for part in &candidate.content.parts {
                    if let PartResponse::Text(json_str) = part {
                        // Clean up the response string
                        let clean_json = json_str
                            .trim()
                            .trim_start_matches("```json")
                            .trim_end_matches("```")
                            .trim();

                        match serde_json::from_str::<GeminiResponse>(clean_json) {
                            Ok(response) => {
                                println!("{} {}", "🤖 →".green(), response.message.yellow());
                                println!(
                                    "{} {}",
                                    "Category : ".green(),
                                    response.category.yellow()
                                );
                                println!("{} {}", "→".blue(), response.command);

                                if !response.dangerous || self.confirm_execution()? {
                                    println!("{}", "Executing...".green());
                                    self.execute_command(&response.command)?;
                                    println!("{}", "Done!".green());
                                } else {
                                    println!("Command execution cancelled.");
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to parse response: {}", e);
                                println!("Raw response: {}", clean_json);

                                if cfg!(debug_assertions) {
                                    println!(
                                        "Debug: Response contains markdown block: {}",
                                        json_str.contains("```")
                                    );
                                    println!("Debug: Cleaned JSON: {}", clean_json);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn confirm_execution(&mut self) -> io::Result<bool> {
        let input = self
            .editor
            .readline(&format!("{} Execute? [y/N]: ", "?".blue()))
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        Ok(input.trim().to_lowercase() == "y")
    }

    fn execute_command(&self, command: &str) -> Result<String, Box<dyn Error>> {
        #[cfg(target_os = "windows")]
        let (program, args) = ("cmd", vec!["/C", command]);

        #[cfg(not(target_os = "windows"))]
        let (program, args) = ("sh", vec!["-c", command]);

        let output = Command::new(program).args(args).output()?;

        io::stdout().write_all(&output.stdout)?;
        io::stderr().write_all(&output.stderr)?;

        if !output.status.success() {
            println!("{} {}", "⚠️ Command failed:".red(), command.yellow());
            println!(
                "{} {}",
                "Exit code:".red(),
                output.status.code().unwrap_or(-1).to_string().yellow()
            );
            let error_message = format!(
                "Command failed with exit code: {}",
                output.status.code().unwrap_or(-1)
            );
            println!("{}", "Requesting AI analysis...".blue());

            let command_clone = command.to_string();
            let error_message_clone = error_message.clone();
            let client_clone = GeminiClient::new(self.config.api_key.clone());

            tokio::spawn(async move {
                let prompt = format!(
                    "The following command failed:\n{}\nwith the following error message:\n{}\nExplain the issue and suggest solutions, WITHOUT markdown formatting or code blocks.",
                    command_clone, error_message_clone
                );

                let req_json = json!({"contents": [{
                        "parts": [{
                            "text": prompt
                        }],
                        "role": "user"
                    }],
                    "tools": []
                });

                let request: GenerateContentRequest = serde_json::from_value(req_json).unwrap();
                if let Ok(response) = client_clone
                    .generate_content("gemini-1.5-flash", &request)
                    .await
                {
                    if let Some(candidates) = response.candidates {
                        for candidate in &candidates {
                            for part in &candidate.content.parts {
                                if let PartResponse::Text(explanation) = part {
                                    println!(
                                        "{} {}",
                                        "🤖 AI Explanation:".green(),
                                        explanation.yellow()
                                    );
                                }
                            }
                        }
                    }
                }
            });

            return Err(error_message.into());
        }
        Ok(String::from_utf8(output.stdout)?)
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        println!("🤖 Welcome to NexSh! Type 'exit' to quit.");

        loop {
            let prompt = format!("{} ", "nexsh>".green());
            match self.editor.readline(&prompt) {
                Ok(line) => {
                    let input = line.trim();
                    if input.is_empty() {
                        continue;
                    }

                    self.editor.add_history_entry(input)?;

                    match input {
                        "exit" | "quit" => break,
                        _ => {
                            if let Err(e) = self.process_command(input).await {
                                eprintln!("{} {}", "error:".red(), e);
                            }
                        }
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    println!("Use 'exit' to quit");
                    continue;
                }
                Err(ReadlineError::Eof) => {
                    break;
                }
                Err(err) => {
                    eprintln!("Error: {}", err);
                    break;
                }
            }
        }

        self.editor.save_history(&self.history_file)?;
        Ok(())
    }
}

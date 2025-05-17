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
use types::{GeminiResponse, Message, NexShConfig};
pub mod prompt;
pub mod types;

#[derive(Parser, Debug)]
#[command(
    name = "nexsh",
    version = "0.5.0",
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
            max_context_messages: 100,
        }
    }
}

pub struct NexSh {
    config: NexShConfig,
    config_dir: PathBuf,
    history_file: PathBuf,
    context_file: PathBuf,
    client: GeminiClient,
    editor: DefaultEditor,
    messages: Vec<Message>,
}

impl NexSh {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let proj_dirs = ProjectDirs::from("com", "gemini-shell", "nexsh")
            .ok_or("Failed to get project directories")?;

        let config_dir = proj_dirs.config_dir().to_path_buf();
        fs::create_dir_all(&config_dir)?;

        let config_file = config_dir.join("nexsh_config.json");
        let history_file = config_dir.join("nexsh_history.txt");
        let context_file = config_dir.join("nexsh_context.json");

        let config = if config_file.exists() {
            let content = fs::read_to_string(&config_file)?;
            let parsed: serde_json::Value = serde_json::from_str(&content)?;
            NexShConfig {
                api_key: parsed
                    .get("api_key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                history_size: parsed
                    .get("history_size")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1000) as usize,
                max_context_messages: parsed
                    .get("max_context_messages")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(100) as usize,
            }
        } else {
            NexShConfig::default()
        };

        let messages = if context_file.exists() {
            let content = fs::read_to_string(&context_file)?;
            serde_json::from_str(&content)?
        } else {
            Vec::new()
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
            context_file,
            client,
            editor,
            messages,
        })
    }

    fn save_config(&self) -> Result<(), Box<dyn Error>> {
        let config_file = self.config_dir.join("nexsh_config.json");
        let content = serde_json::to_string_pretty(&self.config)?;
        fs::write(config_file, content)?;
        Ok(())
    }

    fn save_context(&self) -> Result<(), Box<dyn Error>> {
        let content = serde_json::to_string_pretty(&self.messages)?;
        fs::write(&self.context_file, content)?;
        Ok(())
    }

    fn add_message(&mut self, role: &str, content: &str) {
        let message = Message {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        self.messages.push(message);

        // Trim old messages if we exceed max_context_messages
        if self.messages.len() > self.config.max_context_messages {
            self.messages = self
                .messages
                .split_off(self.messages.len() - self.config.max_context_messages);
        }

        let _ = self.save_context();
    }

    pub fn initialize(&mut self) -> Result<(), Box<dyn Error>> {
        println!("🤖 Welcome to NexSh Setup!");

        self.config.api_key = self
            .editor
            .readline("Enter your Gemini API key: ")?
            .trim()
            .to_string();

        if let Ok(input) = self.editor.readline("Enter history size (default 1000): ") {
            if let Ok(size) = input.trim().parse() {
                self.config.history_size = size;
            }
        }

        if let Ok(input) = self
            .editor
            .readline("Enter max context messages (default 100): ")
        {
            if let Ok(size) = input.trim().parse() {
                self.config.max_context_messages = size;
            }
        }

        self.save_config()?;
        println!("✅ Configuration saved successfully!");

        Ok(())
    }

    pub async fn process_command(&mut self, input: &str) -> Result<(), Box<dyn Error>> {
        if self.config.api_key.is_empty() {
            self.initialize()?;
        }

        let os = std::env::consts::OS.to_string();
        let prompt = SYSTEM_PROMPT.replace("{OS}", &os);

        self.add_message("user", input);

        // Create contents array with history messages in correct format
        let mut contents = Vec::new();

        // Add conversation history
        for msg in &self.messages {
            contents.push(json!({
                "parts": [{
                    "text": msg.content
                }],
                "role": msg.role
            }));
        }

        let req_json = json!({
            "generation_config": {
                "response_mime_type": "application/json",
                "response_schema": {
                    "type": "object",
                    "required": ["message", "command", "dangerous", "category"],
                    "properties": {
                        "message": {
                            "type": "string",
                            "description": "Clear, concise message with relevant emoji",
                            "minLength": 1
                        },
                        "command": {
                            "type": "string",
                            "description": "Shell command to execute, empty if no action needed"
                        },
                        "dangerous": {
                            "type": "boolean",
                            "description": "True if command could be potentially harmful"
                        },
                        "category": {
                            "type": "string",
                            "description": "Classification of the command type",
                            "enum": ["system", "file", "network", "package", "text", "process", "other"]
                        }
                    }
                },
            },
            "system_instruction": {
                "parts": [
                    {
                        "text": prompt
                    }
                ],
                "role": "system"
            },
            "contents": contents,
            "tools": []
        });

        let request: GenerateContentRequest = serde_json::from_value(req_json)?;
        let response = self
            .client
            .generate_content("gemini-2.0-flash", &request)
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
                                if response.command.is_empty() {
                                    // Add model response to context
                                    self.add_message("model", &format!("{}", response.message));
                                    return Ok(());
                                }
                                println!(
                                    "{} {}",
                                    "Category : ".green(),
                                    response.category.yellow()
                                );
                                println!("{} {}", "→".blue(), response.command);
                                self.add_message(
                                    "model",
                                    &format!(
                                        "Command:{}, message:{}",
                                        response.command, response.message
                                    ),
                                );

                                if !response.dangerous || self.confirm_execution()? {
                                    println!("{}", "Executing...".green());
                                    let output = self.execute_command(&response.command)?;
                                    println!("{}", "Done!".green());

                                    // Add command output to context
                                    if !output.is_empty() {
                                        self.add_message(
                                            "model",
                                            &format!("Command output:\n{}", output),
                                        );
                                    }
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
        let _input = self
            .editor
            .readline(&("? Execute? [y/N]: ".red().to_string()))
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        print!("{}️", "⚠️".red());
        if _input.trim() == "N" || _input.trim() == "n" {
            return Ok(false);
        }
        let _input = self
            .editor
            .readline(
                &(" Execute potentially dangerous command? [y/N]: "
                    .red()
                    .to_string()),
            )
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        Ok(_input.trim().to_lowercase() == "y")
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

    fn clear_context(&mut self) -> Result<(), Box<dyn Error>> {
        self.messages.clear();
        self.save_context()?;
        println!("{}", "🧹 Conversation context cleared".green());
        Ok(())
    }

    pub fn print_help(&self) -> Result<(), Box<dyn Error>> {
        println!("🤖 NexSh Help:");
        println!("  - Type 'exit' or 'quit' to exit the shell.");
        println!("  - Type any command to execute it.");
        println!("  - Use 'init' to set up your API key.");
        println!("  - Use 'clear' to clear conversation context.");
        Ok(())
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        println!("🤖 Welcome to NexSh! Type 'exit' to quit.");

        loop {
            let current_dir = std::env::current_dir()?.display().to_string();
            let prompt = format!("{} {} ", current_dir.blue(), "nexsh>".green());
            match self.editor.readline(&prompt) {
                Ok(line) => {
                    let input = line.trim();
                    if input.is_empty() {
                        continue;
                    }

                    self.editor.add_history_entry(input)?;

                    match input {
                        "exit" | "quit" => break,
                        "clear" => self.clear_context()?,
                        "init" => self.initialize()?,
                        "help" => self.print_help()?,
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

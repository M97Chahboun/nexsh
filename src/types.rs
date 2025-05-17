use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NexShConfig {
    pub api_key: String,
    pub history_size: usize,
    pub max_context_messages: usize,
}

#[derive(Debug, Deserialize)]
pub struct GeminiResponse {
    pub message: String,
    pub command: String,
    pub dangerous: bool,
    pub category: String,
}

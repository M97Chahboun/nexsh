use serde::{Deserialize, Serialize};


#[derive(Debug, Serialize, Deserialize)]
pub struct NexShConfig {
    pub api_key: String,
    pub history_size: usize,
    pub default_os: String,
}

#[derive(Debug, Deserialize)]
pub struct GeminiResponse {
    pub message: String,
    pub command: String,
    pub dangerous: bool,
    pub category: String,
}
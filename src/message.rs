use chrono::{SecondsFormat, Utc};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize, Clone)]
pub struct BaseMessage {
    pub id: u32,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    pub content: String,
    #[serde(rename = "chatId")]
    pub chat_id: String,
    #[serde(rename = "messageId")]
    pub message_id: String,
}

impl Default for BaseMessage {
    fn default() -> Self {
        let created_at = {
            let now = Utc::now();
            now.to_rfc3339_opts(SecondsFormat::Micros, true)
        };

        Self {
            id: 0,
            created_at,
            content: String::with_capacity(128),
            chat_id: "".to_string(),
            message_id: "".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "role")]
pub enum Message {
    #[serde(rename = "assistant")]
    AIMessage {
        #[serde(flatten)]
        body: BaseMessage,
        #[serde(rename = "toolCalls")]
        tool_calls: Vec<ToolCall>,
        files: Vec<String>,
    },
    #[serde(rename = "user")]
    UserMessage {
        #[serde(flatten)]
        body: BaseMessage,
    },
    #[serde(rename = "tool_result")]
    ToolCallResult {
        #[serde(flatten)]
        body: BaseMessage,
    },
}

#[derive(Debug, Deserialize, Clone)]
pub struct ToolCall {
    pub name: String,
    pub id: String,
    pub args: Value,
}

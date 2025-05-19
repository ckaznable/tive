use std::ops::Deref;

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
    AIMessage(AIMessage),
    #[serde(rename = "user")]
    UserMessage(UserMessage),
    #[serde(rename = "tool_result")]
    ToolCallResult(ToolCallResult),
}

impl Message {
    pub fn body(&self) -> &BaseMessage {
        match self {
            Message::AIMessage(msg) => &msg.body,
            Message::UserMessage(msg) => &msg.body,
            Message::ToolCallResult(msg) => &msg.body,
        }
    }

    pub fn as_ai_message(&self) -> Option<&AIMessage> {
        match self {
            Message::AIMessage(msg) => Some(msg),
            _ => None,
        }
    }

    pub fn as_user_message(&self) -> Option<&UserMessage> {
        match self {
            Message::UserMessage(msg) => Some(msg),
            _ => None,
        }
    }

    pub fn as_tool_call_result(&self) -> Option<&ToolCallResult> {
        match self {
            Message::ToolCallResult(msg) => Some(msg),
            _ => None,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct ToolCall {
    pub name: String,
    pub id: String,
    pub args: Value,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct AIMessage {
    #[serde(flatten)]
    pub body: BaseMessage,
    #[serde(rename = "toolCalls")]
    pub tool_calls: Vec<ToolCall>,
    pub files: Vec<String>,
}

impl Deref for AIMessage {
    type Target = BaseMessage;

    fn deref(&self) -> &Self::Target {
        &self.body
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct UserMessage {
    #[serde(flatten)]
    pub body: BaseMessage,
}

impl Deref for UserMessage {
    type Target = BaseMessage;

    fn deref(&self) -> &Self::Target {
        &self.body
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ToolCallResult {
    #[serde(flatten)]
    pub body: BaseMessage,
}

impl Deref for ToolCallResult {
    type Target = BaseMessage;

    fn deref(&self) -> &Self::Target {
        &self.body
    }
}

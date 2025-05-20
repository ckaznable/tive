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

impl TryFrom<Message> for AIMessage {
    type Error = anyhow::Error;

    fn try_from(value: Message) -> Result<Self, Self::Error> {
        match value {
            Message::AIMessage(msg) => Ok(msg),
            _ => Err(anyhow::anyhow!("expected ai message")),
        }
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

impl TryFrom<Message> for UserMessage {
    type Error = anyhow::Error;

    fn try_from(value: Message) -> Result<Self, Self::Error> {
        match value {
            Message::UserMessage(msg) => Ok(msg),
            _ => Err(anyhow::anyhow!("expected user message")),
        }
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

#[derive(Debug, Clone, Default)]
pub struct MessageFrame {
    pub ai: AIMessage,
    pub user: UserMessage,
}

impl MessageFrame {
    pub fn split_ref(&self) -> (&UserMessage, &AIMessage) {
        (&self.user, &self.ai)
    }
}

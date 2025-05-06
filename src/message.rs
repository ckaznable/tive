use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct Message {
  id: u32,
  #[serde(rename = "createdAt")]
  created_at: String,
  content: String,
  #[serde(rename = "chatId")]
  chat_id: String,
  #[serde(rename = "messageId")]
  message_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "role")]
pub enum MessageRole {
  #[serde(rename = "assistant")]
  AIMessage {
    #[serde(flatten)]
    body: Message,
    #[serde(rename = "toolCalls")]
    tool_calls: Vec<ToolCall>,
    files: Vec<String>,
  },
  #[serde(rename = "user")]
  UserMessage {
    #[serde(flatten)]
    body: Message,
  },
  #[serde(rename = "tool_result")]
  ToolCallResult {
    #[serde(flatten)]
    body: Message,
  },
}

#[derive(Debug, Deserialize)]
pub struct ToolCall {
  name: String,
  id: String,
  args: Value,
}

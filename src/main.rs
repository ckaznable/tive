use std::collections::HashMap;

use anyhow::Result;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct ChatInfo {
    id: String,
    title: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct MessageInfo {
    #[serde(rename = "userMessageId")]
    user_message_id: String,
    #[serde(rename = "assistantMessageId")]
    assistant_message_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ToolCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ToolResult {
    name: String,
    result: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "content")]
enum ChatResponse {
    #[serde(rename = "text")]
    Text(String),
    #[serde(rename = "tool_calls")]
    ToolCalls(Vec<ToolCall>),
    #[serde(rename = "tool_result")]
    ToolResult(Vec<ToolResult>),
    #[serde(rename = "chat_info")]
    ChatInfo(ChatInfo),
    #[serde(rename = "message_info")]
    MessageInfo(MessageInfo),
}

#[derive(Debug, Serialize, Deserialize)]
struct MessageStreamFrame {
    message: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = Client::new();
    let message = chat(&client, "hi").await?;
    println!("{}", message);
    Ok(())
}

async fn chat(client: &Client, message: &str) -> Result<String> {
    let params = HashMap::from([("message", message)]);
    let mut stream = client.post("http://localhost:61990/api/chat")
        .form(&params)
        .send()
        .await?
        .bytes_stream();

    let mut message = String::with_capacity(1024);
    while let Some(Ok(item)) = stream.next().await {
        if item.is_empty() || item.len() < 6 {
            break;
        }

        let body = item.slice(6..);
        if body.eq_ignore_ascii_case(b"[DONE]\n\n") {
            break;
        }

        let body = serde_json::from_slice::<MessageStreamFrame>(&body)?;
        let body = serde_json::from_str::<ChatResponse>(&body.message)?;
        match body {
            ChatResponse::Text(text) => {
                message.push_str(&text);
            }
            _ => {}
        }
    }

    Ok(message)
}

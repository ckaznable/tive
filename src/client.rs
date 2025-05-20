use std::{
    collections::HashMap,
    pin::Pin,
    time::Duration,
};

use anyhow::Result;
use futures::Stream;
use futures_util::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use tokio::time::timeout;

#[derive(Debug, Deserialize)]
pub struct ChatInfo {
    pub id: String,
    #[serde(rename = "title")]
    pub _title: String,
}

#[derive(Debug, Deserialize)]
pub struct MessageInfo {
    #[serde(rename = "userMessageId")]
    pub user_message_id: String,
    #[serde(rename = "assistantMessageId")]
    pub assistant_message_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Deserialize)]
pub struct ToolResult {
    pub name: String,
    pub result: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "content")]
pub enum ChatResponse {
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

#[derive(Debug, Deserialize)]
struct MessageStreamFrame {
    message: String,
}

pub struct ChatClient {
    client: Client,
    ip: String,
    port: u16,
}

impl ChatClient {
    pub fn new(ip: String, port: u16) -> Self {
        Self {
            client: Client::new(),
            ip,
            port,
        }
    }

    #[inline]
    fn url(&self, path: &str) -> String {
        format!("http://{}:{}/{}", self.ip, self.port, path)
    }

    pub fn chat_stream(&self, message: &str) -> ChatResponseStream {
        let params = HashMap::from([("message", message)]);
        let url = self.url("api/chat");

        let request = self.client.post(url).form(&params);

        let stream = async_stream::try_stream! {
            let response = request.send().await?;
            let mut bytes_stream = response.bytes_stream();

            while let Some(item) = bytes_stream.next().await {
                let item = item?;

                if item.is_empty() || item.len() < 6 {
                    break;
                }

                let body = item.slice(6..);
                if body.eq_ignore_ascii_case(b"[DONE]\n\n") {
                    break;
                }

                let frame = serde_json::from_slice::<MessageStreamFrame>(&body)?;
                let response = serde_json::from_str::<ChatResponse>(&frame.message)?;

                yield response;
            }
        };

        ChatResponseStream {
            stream: Box::pin(stream),
        }
    }

    pub async fn wait_for_server(&self) -> Result<()> {
        let client = self.client.clone();
        let url = self.url("ping");
        let handle = tokio::spawn(async move {
            loop {
                let res = client.get(&url).send().await;
                if let Ok(_) = res {
                    break;
                }

                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        });

        timeout(Duration::from_secs(1), handle).await
            .map_or(Err(anyhow::anyhow!("timeout")), |_| Ok(()))
    }
}

pub struct ChatResponseStream {
    stream: Pin<Box<dyn Stream<Item = Result<ChatResponse, anyhow::Error>> + Send>>,
}

impl Stream for ChatResponseStream {
    type Item = Result<ChatResponse, anyhow::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Self::Item>> {
        self.stream.as_mut().poll_next(cx)
    }
}
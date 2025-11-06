use std::sync::Arc;

use chat::ChatThread;
use tracing::{error, info};
use anyhow::Result;
use client::ChatResponse;
use futures::StreamExt;
use host::{HostEvent, HostListen, HostServer};
use shared::{UIAction, UIActionResult};
use tokio::{signal, sync::mpsc};

mod chat;
mod client;
mod host;
mod logger;
mod message;
mod shared;
mod tui;
mod widget;

#[tokio::main]
async fn main() -> Result<()> {
    logger::initialize_logging()?;

    let (mut host, mut host_recv) = host::HostProcess::new().await?;
    host.spawn().await?;

    let (tx_ui, mut rx_ui) = mpsc::channel(1);
    let (tx_host, rx_host) = mpsc::channel(1);

    let chat_thread = ChatThread::default();
    let (mut chat_writer, chat_reader) = chat_thread.split();

    let tui_handle = tokio::spawn(async move {
        let tui = tui::Tui::new(tx_ui, rx_host);
        tui.run(chat_reader).await;
    });

    // wait for host to return ip and port
    let (ip, port) = tokio::select! {
        _ = signal::ctrl_c() => return Ok(()),
        Some(evt) = host_recv.recv() => {
            match evt {
                HostEvent::BusMessage(msg) => {
                    if let Some(HostServer {
                        listen: Some(HostListen {
                            ip: Some(_ip),
                            port: Some(_port),
                        }),
                    }) = msg.server
                    {
                        info!("Host listen: {}:{}", _ip, _port);
                        (_ip, _port)
                    } else {
                        error!("Failed to get host listen: {:?}", msg);
                        return Err(anyhow::anyhow!("Failed to get host listen"));
                    }
                },
                HostEvent::Error(e) => {
                    return Err(anyhow::anyhow!(e));
                }
            }
        }
    };

    // make sure host is running
    let client = client::ChatClient::new(ip, port);
    client.wait_for_server().await?;

    // main loop
    loop {
        tokio::select! {
            _ = signal::ctrl_c() => break,
            Some(evt) = rx_ui.recv() => {
                match evt {
                    UIAction::Quit => break,
                    UIAction::Chat { id, message } => {
                        info!("Chat: {:?}", id);
                        let mut stream = client.chat_stream(&message, id.as_deref().map(|s| s.as_str()));
                        let mut chat_id: Option<Arc<String>> = id;
                        chat_writer.mut_user_message().content = message;

                        use ChatResponse::*;
                        while let Some(Ok(response)) = stream.next().await {
                            match response {
                                Text(text)=> {
                                    if let Some(id) = &chat_id {
                                        chat_writer.mut_ai_message().content.push_str(&text);
                                        tx_host.send(UIActionResult::Chat {
                                            id: id.clone(),
                                            content: text,
                                        }).await?;
                                    }
                                },
                                ChatInfo(chat_info) => {
                                    if chat_id.is_none() {
                                        chat_writer.mut_user_message().chat_id = chat_info.id.clone();
                                        chat_id = Some(Arc::new(chat_info.id));
                                    }
                                },
                                MessageInfo(message_info) => {
                                    let crate::client::MessageInfo { user_message_id , assistant_message_id } = message_info;
                                    chat_writer.mut_user_message().message_id = user_message_id;
                                    chat_writer.mut_ai_message().message_id = assistant_message_id;
                                },
                                ToolCalls(tool_calls) => {

                                },
                                ToolResult(tool_results) => {

                                },
                            }
                        }

                        chat_writer.flush().await?;
                        tx_host.send(UIActionResult::End).await?;
                    }
                }
            }
        }
    }

    tui_handle.await?;
    Ok(())
}

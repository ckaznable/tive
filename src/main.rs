use std::sync::Arc;

use tracing::info;
use anyhow::Result;
use client::ChatResponse;
use futures::StreamExt;
use host::{HostEvent, HostListen, HostServer};
use shared::{UIAction, UIActionResult};
use tokio::{signal, sync::mpsc};

mod client;
mod host;
mod logger;
mod message;
mod shared;
mod tui;

#[tokio::main]
async fn main() -> Result<()> {
    logger::initialize_logging()?;

    let (mut host, mut host_recv) = host::HostProcess::new().await?;
    host.spawn().await?;

    let (tx_ui, mut rx_ui) = mpsc::channel(1);
    let (tx_host, rx_host) = mpsc::channel(1);

    let tui_handle = tokio::spawn(async move {
        let mut tui = tui::Tui::new(tx_ui, rx_host);
        tui.run().await;
    });

    let (ip, port) = loop {
        tokio::select! {
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
                            break (_ip, _port);
                        } else {
                            return Err(anyhow::anyhow!("Failed to get host listen"));
                        }
                    },
                    HostEvent::Error(e) => {
                        return Err(anyhow::anyhow!(e));
                    }
                }
            }
        }
    };

    let client = client::ChatClient::new(ip, port);
    if let Err(e) = client.wait_for_server().await {
        return Err(e);
    }

    loop {
        tokio::select! {
            _ = signal::ctrl_c() => break,
            Some(evt) = rx_ui.recv() => {
                match evt {
                    UIAction::Quit => break,
                    UIAction::Chat { id, message } => {
                        let mut stream = client.chat_stream(&message);
                        let mut chat_id: Option<Arc<String>> = id.map(Arc::new);

                        use ChatResponse::*;
                        while let Some(Ok(response)) = stream.next().await {
                            match response {
                                Text(text)=> {
                                    if let Some(id) = &chat_id {
                                        tx_host.send(UIActionResult::Chat {
                                            id: id.clone(),
                                            content: text,
                                        }).await?;
                                    }
                                },
                                ChatInfo(chat_info) => {
                                    if chat_id.is_none() {
                                        chat_id = Some(Arc::new(chat_info.id));
                                    }
                                },
                                MessageInfo(message_info) => {
                                    todo!()
                                },
                                ToolCalls(tool_calls) => {
                                    todo!()
                                },
                                ToolResult(tool_results) => {
                                    todo!()
                                },
                            }
                        }

                        tx_host.send(UIActionResult::End).await?;
                    }
                }
            }
        }
    }

    tui_handle.await?;
    Ok(())
}
use anyhow::Result;
use host::{HostEvent, HostListen, HostServer};
use tokio::signal;

mod client;
mod host;
mod tui;

#[tokio::main]
async fn main() -> Result<()> {
    let (mut host, mut host_recv) = host::HostProcess::new().await?;
    host.spawn().await?;

    let tui_handle = tokio::spawn(async move {
        let mut tui = tui::Tui::default();
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

    tui_handle.await?;
    Ok(())
}
use anyhow::Result;
use futures::StreamExt;
use inotify::{EventMask, Inotify, WatchMask};
use nix::{
    sys::signal::{self, Signal},
    unistd::Pid,
};
use std::{
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
};

use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter},
    process::{Child, Command},
    sync::mpsc::{channel, Receiver, Sender},
    task::JoinHandle,
};
use serde::Deserialize;

use crate::shared::PROJECT_DIRS;

pub const COMMAND_ALIAS_FILE: &str = "command_alias.json";
pub const CUSTOM_RULES_FILE: &str = "customrules";
pub const MCP_CONFIG_FILE: &str = "mcp_config.json";
pub const MODEL_CONFIG_FILE: &str = "model_config.json";

#[derive(Debug, Clone, Deserialize)]
pub struct HostStatus {
    state: String,
    last_error: Option<String>,
    error_code: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HostServer {
    pub listen: Option<HostListen>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HostListen {
    pub ip: Option<String>,
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HostMessage {
    pub timestamp: String,
    pub status: HostStatus,
    pub server: Option<HostServer>,
}

#[derive(Debug, Clone)]
pub enum HostEvent {
    BusMessage(HostMessage),
    Error(&'static str),
}

pub struct HostProcess {
    child_process: Option<Child>,
    file_path: PathBuf,
    tx: Sender<HostEvent>,
    watcher_handle: Option<JoinHandle<Result<()>>>,
}

impl HostProcess {
    pub async fn new() -> Result<(Self, Receiver<HostEvent>)> {
        let (tx, rx) = channel(1);
        let file_path = std::env::temp_dir().join("tive").join("bus");

        Ok((
            Self {
                child_process: None,
                file_path,
                tx,
                watcher_handle: None,
            },
            rx,
        ))
    }

    pub async fn spawn(&mut self) -> Result<()> {
        let tx = self.tx.clone();
        let file_path = self.file_path.clone();
        self.watcher_handle = Some(tokio::spawn(async move {
            let mut watcher = FileWatcher::new(tx, &file_path).await?;
            watcher.listen().await?;
            Ok(())
        }));

        let dirs = PROJECT_DIRS.clone();
        let host_config_dir = dirs.host_config_dir();
        let host_data_dir = dirs.host_data_dir();
        let host_cache_dir = dirs.host_cache_dir();
        tokio::fs::create_dir_all(&host_config_dir).await?;
        tokio::fs::create_dir_all(&host_data_dir).await?;
        tokio::fs::create_dir_all(&host_cache_dir).await?;

        Self::clone_host(&host_data_dir).await?;
        self.init_host_config(&host_config_dir, &host_data_dir).await?;

        let process = Command::new("uv")
            .arg("run")
            .arg("dive_httpd")
            .arg("--port")
            .arg("0")
            .arg("--report_status_file")
            .arg(self.file_path.to_string_lossy().to_string())
            .env("PATH", env!("PATH"))
            .env("DIVE_CONFIG_DIR", host_config_dir.to_string_lossy().to_string())
            .env("RESOURCE_DIR", host_cache_dir.to_string_lossy().to_string())
            .stderr(Stdio::null())
            .stdout(Stdio::null())
            .current_dir(&host_data_dir)
            .spawn()?;

        self.child_process = Some(process);
        Ok(())
    }

    async fn init_host_config(&self, config_dir: &Path, db_dir: &Path) -> Result<()> {
        create_file_if_not_exists(&config_dir.join(COMMAND_ALIAS_FILE), b"{}").await?;
        create_file_if_not_exists(&config_dir.join(CUSTOM_RULES_FILE), b"").await?;
        create_file_if_not_exists(&config_dir.join(MCP_CONFIG_FILE), b"{\"mcpServers\":{}}").await?;
        create_file_if_not_exists(&config_dir.join(MODEL_CONFIG_FILE), b"{\"activeProvider\":\"fake\",\"enableTools\":true,\"disableDiveSystemPrompt\":false}").await?;

        let db_path = db_dir.to_string_lossy().to_string();
        create_file_if_not_exists(&config_dir.join("dive_httpd.json"), format!("{{
    \"db\": {{
        \"uri\": \"sqlite:///{}/dived.sqlite\",
        \"pool_size\": 5,
        \"pool_recycle\": 60,
        \"max_overflow\": 10,
        \"echo\": false,
        \"pool_pre_ping\": true,
        \"migrate\": true
        }},
    \"checkpointer\": {{
        \"uri\": \"sqlite:///{}/dived.sqlite\"
    }}
}}",
            &db_path,
            &db_path,
        ).trim().as_bytes()).await?;
        Ok(())
    }

    async fn clone_host(path: &Path) -> std::io::Result<ExitStatus> {
        if !std::fs::exists(path)? {
            Command::new("git")
            .arg("clone")
            .arg("https://github.com/OpenAgentPlatform/dive-mcp-host.git")
            .arg(path)
            .stderr(Stdio::null())
            .stdout(Stdio::null())
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .spawn()?
            .wait()
            .await?;
        }

        Command::new("git")
            .arg("switch")
            .arg("v0.1.5")
            .current_dir(path)
            .stderr(Stdio::null())
            .stdout(Stdio::null())
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .spawn()?
            .wait()
            .await
    }
}

impl Drop for HostProcess {
    fn drop(&mut self) {
        // kill the file watcher
        if let Some(handle) = self.watcher_handle.take() {
            handle.abort();
        }

        // kill the host process
        futures::executor::block_on(async move {
            if let Some(mut child) = self.child_process.take() {
                if let Some(pid) = child.id() {
                    let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
                }

                std::thread::sleep(std::time::Duration::from_millis(300));
                if let Ok(None) = child.try_wait() {
                    let _ = child.kill().await;
                }
            }
        });
    }
}

struct FileWatcher {
    _file: File,
    file_path: PathBuf,
    tx: Sender<HostEvent>,
}

impl Drop for FileWatcher {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.file_path);
    }
}

impl FileWatcher {
    async fn new(tx: Sender<HostEvent>, file_path: &Path) -> Result<Self> {
        tokio::fs::create_dir_all(file_path.parent().unwrap()).await?;
        let _file = File::create(file_path).await?;
        Ok(FileWatcher {
            tx,
            _file,
            file_path: file_path.to_path_buf(),
        })
    }

    async fn listen(&mut self) -> Result<()> {
        let inotify = Inotify::init()?;
        inotify
            .watches()
            .add(&self.file_path, WatchMask::MODIFY)?;

        let mut buffer = [0u8; 1024];
        let mut buf_file = vec![0u8; 1024];
        let mut stream = inotify.into_event_stream(&mut buffer)?;
        while let Some(Ok(event)) = stream.next().await {
            if event.mask.contains(EventMask::ISDIR) {
                continue;
            }

            if let Ok(file) = File::open(&self.file_path).await {
                let mut reader = BufReader::new(file);
                match reader.read(&mut buf_file).await {
                    Ok(n) if n > 0 => {
                        if let Ok(message) = serde_json::from_slice::<HostMessage>(&buf_file[..n]) {
                            let _ = self.tx.send(HostEvent::BusMessage(message)).await;
                        }
                    }
                    Err(_) => {
                        let _ = self.tx.send(HostEvent::Error("Failed to read host message")).await;
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }
}

async fn create_file_if_not_exists(path: &Path, content: &[u8]) -> Result<()> {
    if !tokio::fs::try_exists(path).await? {
        let file = File::create(path).await?;
        let mut writer = BufWriter::new(file);
        writer.write_all(content).await?;
        writer.flush().await?;
    }

    Ok(())
}

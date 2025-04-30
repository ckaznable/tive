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

use directories::{ProjectDirs, UserDirs};
use tokio::{
    fs::File,
    io::{AsyncReadExt, BufReader},
    process::{Child, Command},
    sync::mpsc::{channel, Receiver, Sender},
    task::JoinHandle,
};
use serde::Deserialize;

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

        let (config_dir, data_dir) = Self::get_config_dir();
        tokio::fs::create_dir_all(&config_dir).await?;
        tokio::fs::create_dir_all(&data_dir).await?;

        let host_dir = data_dir.join("host");
        Self::clone_host(&host_dir).await?;

        let process = Command::new("uv")
            .arg("run")
            .arg("dive_httpd")
            .arg("--port")
            .arg("0")
            .arg("--report_status_file")
            .arg(self.file_path.to_string_lossy().to_string())
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .stderr(Stdio::null())
            .stdout(Stdio::null())
            .current_dir(&host_dir)
            .spawn()?;

        self.child_process = Some(process);
        Ok(())
    }

    fn get_config_dir() -> (PathBuf, PathBuf) {
        ProjectDirs::from("", "", "tive")
            .map(|proj_dirs| (
                proj_dirs.config_dir().to_path_buf(),
                proj_dirs.data_local_dir().to_path_buf()
            ))
            .or_else(|| {
                UserDirs::new()
                    .map(|user| {
                        let home = user.home_dir();
                        (
                            home.join(".config").join("tive"),
                            home.join(".local").join("share").join("tive")
                        )
                    })
            })
            .unwrap_or_else(|| {
                (PathBuf::from("./.tive/config"), PathBuf::from("./.tive/data"))
            })
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
            .arg("v0.1.4")
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
                    let _ = child.kill();
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

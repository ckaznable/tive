use std::{
    path::PathBuf,
    sync::{Arc, LazyLock},
};

use directories::{ProjectDirs, UserDirs};

pub static PROJECT_NAME: LazyLock<String> = LazyLock::new(|| env!("CARGO_CRATE_NAME").to_uppercase().to_string());
pub static PROJECT_DIRS: LazyLock<Dirs> = LazyLock::new(|| {
    ProjectDirs::from("", "", "tive")
        .map(|proj_dirs| Dirs {
            config: proj_dirs.config_dir().to_path_buf(),
            data: proj_dirs.data_local_dir().to_path_buf(),
            cache: proj_dirs.cache_dir().to_path_buf(),
        })
        .or_else(|| {
            UserDirs::new()
                .map(|user| {
                    let home = user.home_dir();
                    Dirs {
                        config: home.join(".config").join("tive"),
                        data: home.join(".local").join("share").join("tive"),
                        cache: home.join(".cache").join("tive"),
                    }
                })
        })
        .unwrap_or_else(|| {
            Dirs {
                config: PathBuf::from("./.tive/config"),
                data: PathBuf::from("./.tive/data"),
                cache: PathBuf::from("./.tive/cache"),
            }
        })
});

#[derive(Debug, Clone)]
pub struct Dirs {
    pub config: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
}

impl Dirs {
    pub fn host_config_dir(&self) -> PathBuf {
        self.config.join("host")
    }

    pub fn host_data_dir(&self) -> PathBuf {
        self.data.join("host")
    }

    pub fn host_cache_dir(&self) -> PathBuf {
        self.cache.join("host")
    }
}

pub enum UIAction {
    Quit,
    Chat {
        id: Option<String>,
        message: String,
    },
}

pub enum UIActionResult {
    End,
    Chat {
        id: Arc<String>,
        content: String,
    },
}
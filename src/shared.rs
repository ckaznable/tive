use std::sync::Arc;

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
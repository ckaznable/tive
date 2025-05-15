use std::{ops::Deref, sync::Arc};

use anyhow::{anyhow, Result};
use crate::{message::{BaseMessage, Message}};
use tokio::sync::{mpsc, Mutex};

#[derive(Debug, Clone)]
pub struct ChatThread {
    inner: Arc<Mutex<ChatThreadInner>>,
}

impl Default for ChatThread {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ChatThreadInner::default())),
        }
    }
}

impl ChatThread {
    pub fn split(self) -> (ChatWriter, ChatReader) {
        let (tx, rx) = mpsc::channel(1);
        (
            ChatWriter {
                notify_reader_tx: tx,
                thread: self.inner.clone(),
                user_message: None,
                ai_message: None,
                thread_id: None,
            },
            ChatReader {
                thread: self.inner.clone(),
                thread_id: None,
                messages: vec![],
                notify_update_rx: rx,
            }
        )
    }
}

impl Deref for ChatThread {
    type Target = Mutex<ChatThreadInner>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[derive(Debug, Default)]
pub struct ChatThreadInner {
    pub id: Option<String>,
    pub messages: Vec<Arc<Message>>,
}

#[derive(Debug, Clone)]
pub struct ChatWriter {
    thread: Arc<Mutex<ChatThreadInner>>,
    notify_reader_tx: mpsc::Sender<()>,
    pub user_message: Option<BaseMessage>,
    pub ai_message: Option<BaseMessage>,
    pub thread_id: Option<String>,
}

impl ChatWriter {
    pub fn mut_user_message(&mut self) -> &mut BaseMessage {
        self.user_message.get_or_insert_default()
    }

    pub fn mut_ai_message(&mut self) -> &mut BaseMessage {
        self.ai_message.get_or_insert_default()
    }

    pub async fn flush(&mut self) -> Result<(Option<Arc<Message>>, Option<Arc<Message>>)> {
        let mut thread = self.thread.lock().await;

        thread.id = self.thread_id.take();

        let user_message = self.user_message.take()
            .map(|msg| Message::UserMessage { body: msg })
            .map(Arc::new);
        let ai_message = self.ai_message.take()
            .map(|msg| Message::AIMessage { body: msg, tool_calls: vec![], files: vec![] })
            .map(Arc::new);

        if let Some(user_message) = user_message.clone() {
            thread.messages.push(user_message);
        }

        if let Some(ai_message) = ai_message.clone() {
            thread.messages.push(ai_message);
        }

        let _ = self.notify_reader_tx.try_send(());
        Ok((user_message, ai_message))
    }
}

#[derive(Debug)]
pub struct ChatReader {
    thread: Arc<Mutex<ChatThreadInner>>,
    thread_id: Option<String>,
    messages: Vec<Arc<Message>>,
    notify_update_rx: mpsc::Receiver<()>,
}

impl ChatReader {
    pub async fn read(&mut self) -> Result<&[Arc<Message>]> {
        let updated = match self.notify_update_rx.try_recv() {
            Ok(_) => true,
            Err(mpsc::error::TryRecvError::Empty) => false,
            Err(mpsc::error::TryRecvError::Disconnected) => {
                return Err(anyhow!("Writer closed"));
            }
        };

        if updated {
            let thread = self.thread.lock().await;
            self.messages.clear();
            self.messages.extend(thread.messages.iter().cloned());
        }

        Ok(&self.messages)
    }
}
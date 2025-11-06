use std::{
    ops::Deref,
    sync::{
        atomic::{
            AtomicBool,
            Ordering
        },
        Arc
    },
};

use tracing::info;
use anyhow::Result;
use crate::message::{AIMessage, BaseMessage, Message, MessageFrame, UserMessage};
use tokio::sync::{Mutex};

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
        let update_flag = Arc::new(AtomicBool::new(false));
        (
            ChatWriter {
                update_flag: update_flag.clone(),
                thread: self.inner.clone(),
                user_message: None,
                ai_message: None,
                thread_id: None,
            },
            ChatReader {
                thread: self.inner.clone(),
                thread_id: None,
                messages: vec![],
                update_flag: update_flag.clone(),
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
    pub messages: Vec<Arc<MessageFrame>>,
}

#[derive(Debug, Clone)]
pub struct ChatWriter {
    thread: Arc<Mutex<ChatThreadInner>>,
    update_flag: Arc<AtomicBool>,
    pub user_message: Option<BaseMessage>,
    pub ai_message: Option<BaseMessage>,
    pub thread_id: Option<String>,
}

impl ChatWriter {
    #[inline]
    pub fn mut_user_message(&mut self) -> &mut BaseMessage {
        self.user_message.get_or_insert_default()
    }

    #[inline]
    pub fn mut_ai_message(&mut self) -> &mut BaseMessage {
        self.ai_message.get_or_insert_default()
    }

    pub async fn flush(&mut self) -> Result<Arc<MessageFrame>> {
        let mut thread = self.thread.lock().await;

        thread.id = self.thread_id.take();

        let user_message: Option<UserMessage> = self.user_message.take()
            .map(|msg| Message::UserMessage(UserMessage { body: msg }))
            .and_then(|msg| msg.try_into().ok());
        let ai_message: Option<AIMessage> = self.ai_message.take()
            .map(|msg| Message::AIMessage(AIMessage { body: msg, tool_calls: vec![], files: vec![] }))
            .and_then(|msg| msg.try_into().ok());

        let (Some(user_message), Some(ai_message)) = (user_message.clone(), ai_message.clone()) else {
            return Ok(Arc::new(MessageFrame::default()));
        };

        let frame = Arc::new(MessageFrame {
            ai: ai_message,
            user: user_message,
        });

        info!("flushed frame: user: {:?}, ai: {:?}", frame.user.id, frame.ai.id);
        thread.messages.push(frame.clone());
        self.update_flag.store(true, Ordering::Release);
        Ok(frame)
    }
}

#[derive(Debug)]
pub struct ChatReader {
    thread: Arc<Mutex<ChatThreadInner>>,
    thread_id: Option<String>,
    messages: Vec<Arc<MessageFrame>>,
    update_flag: Arc<AtomicBool>,
}

impl ChatReader {
    pub async fn read(&mut self) -> &[Arc<MessageFrame>] {
        let updated = self.update_flag.load(Ordering::Relaxed);

        if updated {
            self.update_flag.store(false, Ordering::Release);
            let thread = self.thread.lock().await;
            self.messages.clear();
            self.messages.extend(thread.messages.iter().cloned());
        }

        &self.messages
    }
}

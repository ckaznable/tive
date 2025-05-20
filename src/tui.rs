use std::sync::Arc;

use tracing::info;
use crossterm::event::{EventStream, KeyEvent, KeyModifiers};
use ratatui::{
    crossterm::event::{ Event, KeyCode },
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders},
};
use futures_util::{FutureExt, StreamExt};
use ratatui::{
    layout::{Constraint, Layout},
    Frame,
};
use tokio::sync::mpsc::{Receiver, Sender};
use tui_textarea::TextArea;

use crate::{
    chat::ChatReader,
    message::{AIMessage, MessageFrame, UserMessage},
    shared::{UIAction, UIActionResult},
    widget::{self, message::MessageState, status_bar::StatusBar},
};

#[derive(Debug, Default, Clone, Copy)]
pub enum InputMode {
    #[default]
    Normal,
    Insert,
}

#[derive(Debug)]
pub struct Tui<'a> {
    mode: InputMode,
    quit: bool,
    input: TextArea<'a>,
    tx: Sender<UIAction>,
    rx: Receiver<UIActionResult>,
    user_message: UserMessage,
    ai_message: AIMessage,
    streaming: bool,
    chat_reader: Option<ChatReader>,
    message_state: Option<MessageState>,
    ct_index: usize,
    thread_len: usize,
}

impl<'a> Tui<'a> {
    pub fn new(tx: Sender<UIAction>, rx: Receiver<UIActionResult>, chat_reader: ChatReader) -> Self {
        Self {
            tx,
            rx,
            chat_reader: Some(chat_reader),
            quit: false,
            mode: InputMode::default(),
            input: TextArea::default(),
            user_message: UserMessage::default(),
            ai_message: AIMessage::default(),
            streaming: false,
            message_state: None,
            ct_index: 0,
            thread_len: 0,
        }
    }

    pub async fn run(mut self) {
        let mut terminal = ratatui::init();
        let mut reader = EventStream::new();

        let frame = terminal.get_frame();
        let [chat_viewport, _, _] = layout(frame.area());
        self.message_state = Some(MessageState::new(chat_viewport));

        let mut cr = self.chat_reader.take().unwrap();

        loop {
            if self.quit {
                let _ = self.tx.send(UIAction::Quit).await;
                break;
            }

            // style for input
            self.tick_input_state();

            // get current chat to render to viewport
            let ct = cr.read().await;
            self.thread_len = ct.len();

            terminal.draw(|f| draw(f, &mut self, ct)).expect("failed to draw frame");

            let crossterm_event = reader.next().fuse();
            tokio::select! {
                Some(e) = crossterm_event => {
                    match e {
                        Ok(evt) => {
                            self.handle_input_event(evt).await;
                        },
                        _ => (),
                    }
                },
                Some(evt) = self.rx.recv() => {
                    use UIActionResult::*;
                    match evt {
                        Chat { content, .. } => {
                            self.ai_message.body.content.push_str(&content);
                        },
                        End => {
                            self.streaming = false;
                            self.ct_index = if self.ct_index > 0 { self.ct_index.saturating_add(1) } else { 0 };
                        },
                    }
                },
            }
        }
    }

    fn tick_input_state(&mut self) {
        let style = if self.streaming {
            Style::default().fg(Color::LightGreen)
        } else {
            match self.mode {
                InputMode::Normal => Style::default().fg(Color::Blue),
                InputMode::Insert => Style::default().fg(Color::LightGreen),
            }
        };

        self.set_input_block(self.get_input_block(style));
    }

    #[inline]
    fn get_input_block(&self, style: Style) -> Block<'a> {
        Block::default()
            .borders(Borders::ALL)
            .border_style(style)
            .border_type(BorderType::Rounded)
            .title("Chat")
    }

    #[inline]
    fn set_input_block(&mut self, block: Block<'a>) {
        self.input.set_block(block);
    }

    #[inline]
    async fn handle_input_event(&mut self, event: Event) {
        match event {
            Event::Key(e) => match self.mode {
                InputMode::Normal => self.handle_normal_key_event(e).await,
                InputMode::Insert => self.handle_insert_key_event(e).await,
            }
            _ => (),
        }
    }

    async fn handle_normal_key_event(&mut self, event: KeyEvent) {
        match event.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('i' | 'a') => {
                if !self.streaming {
                    self.mode = InputMode::Insert;
                }
            }
            KeyCode::Char('j') => {
                self.message_state.as_mut().unwrap().scroll_down();
            }
            KeyCode::Char('k') => {
                self.message_state.as_mut().unwrap().scroll_up();
            }
            KeyCode::Char('n') if event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.ct_index = self.ct_index.saturating_sub(1);
            }
            KeyCode::Char('p') if event.modifiers.contains(KeyModifiers::CONTROL) => {
                let index = self.ct_index.saturating_add(1);
                if index < self.thread_len {
                    self.ct_index = index;
                }
            }
            _ => (),
        }
    }

    async fn handle_insert_key_event(&mut self, event: KeyEvent) {
        match event.code {
            KeyCode::Esc => self.mode = InputMode::Normal,
            KeyCode::Enter => {
                if self.streaming {
                    return;
                }

                if self.input.is_empty() {
                    return;
                }

                self.ai_message.body.content.clear();
                self.user_message.body.content.clear();
                self.user_message.body.content.push_str(&self.input.lines().join("\n"));
                self.streaming = true;

                let tx = self.tx.clone();
                let message = self.input.lines().join("\n");
                self.input = TextArea::default();
                tokio::spawn(async move {
                    let _ = tx.send(UIAction::Chat {
                        id: None,
                        message,
                    }).await;
                });
            },
            _ => {
                self.input.input(event);
            },
        }
    }
}

impl<'a> Drop for Tui<'a> {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

#[inline]
fn get_chat_to_render<'b>(streaming: bool, index: usize, def: (&'b UserMessage, &'b AIMessage), ct: &'b [Arc<MessageFrame>]) -> (&'b UserMessage, &'b AIMessage) {
    if ct.len() < 2 {
        return def;
    }

    if streaming && index == 0 {
        return def;
    }

    let index = if index >= ct.len() { ct.len() - 1 } else { index };
    let Some(msg) = ct.get(ct.len() - index - 1) else {
        info!("ct index out of bounds");
        return def;
    };

    msg.split_ref()
}

#[inline]
fn layout(area: Rect) -> [Rect; 3] {
    let [chat, input, status_bar] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(5),
        Constraint::Length(2),
    ])
    .areas(area);

    [chat, input, status_bar]
}

fn draw(frame: &mut Frame, state: &mut Tui, current_ct: &[Arc<MessageFrame>]) {
    let area = frame.area();

    // prepare message state
    let msg_state = state.message_state.as_mut().unwrap();
    msg_state.set_viewport(area);
    let chat_buf = (&state.user_message, &state.ai_message);
    let (user, ai) = get_chat_to_render(state.streaming, state.ct_index, chat_buf, current_ct);
    msg_state.pre_render(user, ai);

    let [chat, input, status_bar] = layout(area);

    frame.render_widget(&state.input, input);
    frame.render_widget(StatusBar { mode: state.mode }, status_bar);
    frame.render_stateful_widget_ref(widget::message::Message, chat, state.message_state.as_mut().unwrap());
}
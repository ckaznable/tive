use std::{io::stdout, sync::Arc, time::Duration};

use anyhow::Result;
use tracing::{error, info};
use crossterm::{
    event::{EventStream, KeyEvent, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    crossterm::event::{ Event, KeyCode },
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders}, DefaultTerminal,
};
use futures_util::{FutureExt, StreamExt};
use ratatui::{
    layout::{Constraint, Layout},
    Frame,
};
use tokio::{process::Command, sync::mpsc::{Receiver, Sender}};
use tui_textarea::TextArea;

use crate::{
    chat::ChatReader,
    host::{MCP_CONFIG_FILE, MODEL_CONFIG_FILE},
    message::{AIMessage, MessageFrame, UserMessage},
    shared::{UIAction, UIActionResult, PROJECT_DIRS},
    widget::{message::{Message, MessageState}, status_bar::StatusBar},
};

#[derive(Debug, Default, Clone, Copy)]
pub enum InputMode {
    #[default]
    Normal,
    Insert,
    Leader,
    EditFile,
}

#[derive(Debug, Clone)]
enum TuiInnerAction {
    OpenEditor(String),
    ForceRender,
}

#[derive(Debug)]
pub struct Tui<'a> {
    mode: InputMode,
    quit: bool,
    input: TextArea<'a>,
    tx: Sender<UIAction>,
    rx: Receiver<UIActionResult>,
    inner_tx: Sender<TuiInnerAction>,
    inner_rx: Receiver<TuiInnerAction>,
    user_message: UserMessage,
    ai_message: AIMessage,
    streaming: bool,
    message_state: Option<MessageState>,
    ct_index: usize,
    thread_len: usize,
    chat_id: Option<Arc<String>>,
}

impl<'a> Tui<'a> {
    pub fn new(tx: Sender<UIAction>, rx: Receiver<UIActionResult>) -> Self {
        let (inner_tx, inner_rx) = tokio::sync::mpsc::channel(1);

        Self {
            tx,
            rx,
            inner_tx,
            inner_rx,
            quit: false,
            mode: InputMode::default(),
            input: TextArea::default(),
            user_message: UserMessage::default(),
            ai_message: AIMessage::default(),
            streaming: false,
            message_state: None,
            ct_index: 0,
            thread_len: 0,
            chat_id: None,
        }
    }

    pub async fn run(mut self, mut cr: ChatReader) {
        let mut terminal = ratatui::init();
        let mut reader = EventStream::new();

        let frame = terminal.get_frame();
        let [chat_viewport, _, _] = layout(frame.area());
        self.message_state = Some(MessageState::new(chat_viewport));

        // 60 fps
        let mut animation_timer = tokio::time::interval(Duration::from_micros((1000. / 60. * 1000.) as u64));
        let mut tick_by_animation = false;
        let mut last_animation_frame = false;

        loop {
            if self.quit {
                let _ = self.tx.send(UIAction::Quit).await;
                break;
            }

            let animation = self.streaming;
            let clean_frame = !animation && last_animation_frame;
            let mut anima_tick = false;

            // style for input
            self.tick_input_state();

            // get current chat to render to viewport
            let ct = cr.read().await;
            self.thread_len = ct.len();

            if (animation && tick_by_animation) || (!animation && !tick_by_animation) || clean_frame {
                terminal.draw(|f| draw(f, &mut self, ct)).expect("failed to draw frame");
            }

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
                _ = animation_timer.tick() => {
                    anima_tick = true;
                },
                Some(evt) = self.rx.recv() => {
                    use UIActionResult::*;
                    match evt {
                        Chat { content, id } => {
                            self.ai_message.body.content.push_str(&content);
                            self.chat_id = Some(id);
                        },
                        End => {
                            self.streaming = false;
                            self.ct_index = if self.ct_index > 0 { self.ct_index.saturating_add(1) } else { 0 };
                            let _ = self.inner_tx.send(TuiInnerAction::ForceRender).await;
                        },
                    }
                },
                Some(evt) = self.inner_rx.recv() => {
                    use TuiInnerAction::*;
                    match evt {
                        OpenEditor(path) => {
                            info!("opening editor: {}", path);
                            if self.open_editor(&mut terminal, path).await.is_err() {
                                error!("failed to open editor");
                            };
                        }
                        ForceRender => {
                            info!("force render");
                        }
                    }
                },
            }

            if (animation && !anima_tick) || clean_frame {
                animation_timer.tick().await;
                tick_by_animation = true;
            } else {
                tick_by_animation = anima_tick;
            }
            last_animation_frame = animation;
        }
    }

    async fn open_editor(&mut self, terminal: &mut DefaultTerminal, path: String) -> Result<()> {
        stdout().execute(LeaveAlternateScreen)?;
        disable_raw_mode()?;
        let editor = std::env::var("EDITOR").unwrap_or("vim".to_string());
        let status = Command::new(editor).arg(path).status().await?;
        info!("editor exited with status: {}", status);
        stdout().execute(EnterAlternateScreen)?;
        enable_raw_mode()?;
        terminal.clear()?;
        Ok(())
    }

    fn tick_input_state(&mut self) {
        let style = if self.streaming {
            Style::default().fg(Color::LightGreen)
        } else {
            match self.mode {
                InputMode::Insert => Style::default().fg(Color::LightGreen),
                _ => Style::default().fg(Color::Blue),
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
                InputMode::Insert => self.handle_insert_key_event(e).await,
                InputMode::Leader => self.handle_leader_key_event(e).await,
                InputMode::EditFile => self.handle_edit_file_key_event(e).await,
                _ => self.handle_normal_key_event(e).await,
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
                self.message_state.as_mut().unwrap().reset();
                self.ct_index = self.ct_index.saturating_sub(1);
            }
            KeyCode::Char('p') if event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.message_state.as_mut().unwrap().reset();
                let index = self.ct_index.saturating_add(1);
                if index < self.thread_len {
                    self.ct_index = index;
                }
            }
            KeyCode::Char(' ') => {
                self.mode = InputMode::Leader;
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

                self.message_state.as_mut().unwrap().reset();
                self.ai_message.body.content.clear();
                self.user_message.body.content.clear();
                self.user_message.body.content.push_str(&self.input.lines().join("\n"));
                self.streaming = true;
                self.mode = InputMode::Normal;

                let tx = self.tx.clone();
                let message = self.input.lines().join("\n");
                self.input = TextArea::default();

                let id = self.chat_id.clone();
                tokio::spawn(async move {
                    let _ = tx.send(UIAction::Chat { id, message }).await;
                });
            },
            _ => {
                self.input.input(event);
            },
        }
    }

    async fn handle_leader_key_event(&mut self, event: KeyEvent) {
        match event.code {
            KeyCode::Char('e') => {
                self.mode = InputMode::EditFile;
            }
            _ => {
                self.mode = InputMode::Normal;
                self.handle_normal_key_event(event).await;
            },
        }
    }

    async fn handle_edit_file_key_event(&mut self, event: KeyEvent) {
        self.mode = InputMode::Normal;
        match event.code {
            KeyCode::Char('m') => {
                let _ = self.inner_tx.send(TuiInnerAction::OpenEditor(PROJECT_DIRS.host_config_dir().join(MODEL_CONFIG_FILE).to_string_lossy().to_string())).await;
            }
            KeyCode::Char('s') => {
                let _ = self.inner_tx.send(TuiInnerAction::OpenEditor(PROJECT_DIRS.host_config_dir().join(MCP_CONFIG_FILE).to_string_lossy().to_string())).await;
            }
            _ => {
                self.handle_normal_key_event(event).await;
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
    if ct.is_empty() {
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

    let [chat, input, status_bar] = layout(area);

    // prepare message state
    let msg_state = state.message_state.as_mut().unwrap();
    msg_state.set_viewport(chat);
    let chat_buf = (&state.user_message, &state.ai_message);
    let (user, ai) = get_chat_to_render(state.streaming, state.ct_index, chat_buf, current_ct);
    msg_state.pre_render(user, ai);

    frame.render_widget(&state.input, input);
    frame.render_widget(StatusBar { mode: state.mode }, status_bar);
    frame.render_stateful_widget_ref(Message { streaming: state.streaming }, chat, msg_state);
}

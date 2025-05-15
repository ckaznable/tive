use crossterm::event::EventStream;
use ratatui::{
    crossterm::event::{Event, KeyCode},
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
    chat::ChatReader, shared::{UIAction, UIActionResult}, widget::status_bar::StatusBar
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
    text: String,
    streaming: bool,
    chat_reader: ChatReader,
}

impl<'a> Tui<'a> {
    pub fn new(tx: Sender<UIAction>, rx: Receiver<UIActionResult>, chat_reader: ChatReader) -> Self {
        Self {
            tx,
            rx,
            chat_reader,
            quit: false,
            mode: InputMode::default(),
            input: TextArea::default(),
            text: String::with_capacity(1024),
            streaming: false,
        }
    }

    pub async fn run(&mut self) {
        let mut terminal = ratatui::init();
        let mut reader = EventStream::new();

        loop {
            if self.quit {
                let _ = self.tx.send(UIAction::Quit).await;
                break;
            }

            // style for input
            self.tick_input_state();

            terminal.draw(|f| Self::draw(f, &self)).expect("failed to draw frame");

            let crossterm_event = reader.next().fuse();
            tokio::select! {
                Some(e) = crossterm_event => {
                    match e {
                        Ok(evt) => {
                            self.handle_input_event(evt);
                        },
                        _ => (),
                    }
                },
                Some(evt) = self.rx.recv() => {
                    use UIActionResult::*;
                    match evt {
                        Chat { content, .. } => {
                            self.text.push_str(&content);
                        },
                        End => {
                            self.streaming = false;
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
    fn handle_input_event(&mut self, event: Event) {
        match self.mode {
            InputMode::Normal => self.handle_normal_key_event(event),
            InputMode::Insert => self.handle_insert_key_event(event),
        }
    }

    fn handle_normal_key_event(&mut self, event: Event) {
        let Event::Key(event) = event else {
            return;
        };

        match event.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('i' | 'a') => {
                if !self.streaming {
                    self.mode = InputMode::Insert;
                }
            },
            _ => (),
        }
    }

    fn handle_insert_key_event(&mut self, _event: Event) {
        let Event::Key(event) = _event else {
            return;
        };

        match event.code {
            KeyCode::Esc => self.mode = InputMode::Normal,
            KeyCode::Enter => {
                if self.input.is_empty() {
                    return;
                }

                self.text.clear();
                self.streaming = true;
                self.mode = InputMode::Normal;

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

    fn draw(frame: &mut Frame, s: &Self) {
        let [chat, input, status_bar] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(5),
            Constraint::Length(2),
        ])
        .areas(frame.area());

        frame.render_widget(&s.input, input);
        frame.render_widget(&s.text, chat);
        frame.render_widget(StatusBar { mode: s.mode }, status_bar);
    }
}

impl<'a> Drop for Tui<'a> {
    fn drop(&mut self) {
        ratatui::restore();
    }
}
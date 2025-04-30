
use crossterm::event::EventStream;
use ratatui::crossterm::event::{Event, KeyCode};
use futures_util::{FutureExt, StreamExt};
use ratatui::{layout::{Constraint, Layout}, Frame};
use tui_input::{backend::crossterm::EventHandler, Input};

#[derive(Debug, Default)]
pub enum InputMode {
    #[default]
    Normal,
    Insert,
}

#[derive(Debug, Default)]
pub struct Tui {
    mode: InputMode,
    quit: bool,
    input: Input,
}

impl Tui {
    pub async fn run(&mut self) {
        let mut terminal = ratatui::init();
        let mut reader = EventStream::new();

        loop {
            if self.quit {
                break;
            }

            let crossterm_event = reader.next().fuse();

            terminal.draw(Self::draw).expect("failed to draw frame");

            tokio::select! {
                Some(e) = crossterm_event => {
                    match e {
                        Ok(evt) => {
                            self.handle_input_event(evt);
                        },
                        _ => (),
                    }
                },
            }
        }
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
            KeyCode::Char('i') => self.mode = InputMode::Insert,
            _ => (),
        }
    }

    fn handle_insert_key_event(&mut self, _event: Event) {
        let Event::Key(event) = _event else {
            return;
        };

        match event.code {
            KeyCode::Esc => self.mode = InputMode::Normal,
            _ => {
                self.input.handle_event(&_event);
            },
        }
    }

    fn draw(frame: &mut Frame) {
        let [chat, input] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(frame.area());
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        ratatui::restore();
    }
}
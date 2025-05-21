use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{
        Block,
        BorderType,
        Borders,
        Paragraph,
        StatefulWidget,
        StatefulWidgetRef,
        Widget,
    },
};

use crate::message;

const ANIMATION_CHAR_TOP: char = '>';
const ANIMATION_CHAR_BOTTOM: char = '<';
const ANIMATION_CHAR_LEFT: char = 'A';
const ANIMATION_CHAR_RIGHT: char = 'V';

#[derive(Debug, Clone, Copy)]
enum MessageBorderAnimationPos {
    Top(u16),
    Bottom(u16),
    Left(u16),
    Right(u16),
}

#[derive(Debug, Clone)]
struct MessageAnimation {
    pos: MessageBorderAnimationPos,
    frame: u8,
}

impl Default for MessageAnimation {
    fn default() -> Self {
        Self {
            pos: MessageBorderAnimationPos::Top(0),
            frame: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MessageState {
    buf: Buffer,
    scroll_y: u16,
    viewport: Rect,
    animation: MessageAnimation,
}

impl MessageState {
    pub fn new(viewport: Rect) -> Self {
        Self {
            buf: Buffer::default(),
            scroll_y: 0,
            viewport,
            animation: MessageAnimation::default(),
        }
    }

    pub fn reset(&mut self) {
        self.scroll_y = 0;
        self.buf.reset();
    }

    pub fn set_viewport(&mut self, viewport: Rect) {
        self.viewport = viewport;
    }

    pub fn scroll_up(&mut self) {
        self.scroll_y = self.scroll_y.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        let scroll_y = self.scroll_y.saturating_add(1);
        if scroll_y + self.viewport.height <= self.buf.area().height {
            self.scroll_y = scroll_y;
        }
    }

    pub fn draw(&mut self, area: Rect, buf: &mut Buffer) {
        let Rect { x, y, width, height } = area;
        for h in 0..height {
            for w in 0..width {
                let Some(_cell) = self.buf.cell((w, h + self.scroll_y)) else {
                    continue;
                };
                if let Some(cell) = buf.cell_mut((x + w, y + h)) {
                    *cell = _cell.clone();
                };
            }
        }
    }

    pub fn pre_render(&mut self, user_message: &message::UserMessage, ai_message: &message::AIMessage) {
        if user_message.body.content.is_empty() && ai_message.body.content.is_empty() {
            return;
        }

        let opt = textwrap::Options::new(self.viewport.width as usize).word_separator(textwrap::WordSeparator::AsciiSpace);
        let user_lines = textwrap::wrap(&user_message.body.content, &opt);
        let ai_lines = textwrap::wrap(&ai_message.body.content, opt);
        // +2 for the `you >` and `ai >` lines
        // +1 for the space
        let total_lines = user_lines.len() + ai_lines.len() + 3;

        let rect = if total_lines >= self.viewport.height as usize {
            Rect::new(self.viewport.x, self.viewport.y, self.viewport.width, total_lines as u16)
        } else {
            self.viewport
        };

        self.buf.resize(rect);

        let mut text = Vec::with_capacity(total_lines);
        text.push(Line::from("You >".blue()));
        text.extend(user_lines.iter().map(|line| line.as_ref().into()));
        text.push("".into());
        text.push(Line::from("AI >".green()));
        text.extend(ai_lines.iter().map(|line| line.as_ref().into()));

        Paragraph::new(text).render(rect, &mut self.buf);
    }

    pub fn render_border_animation(&mut self, area: Rect, buf: &mut Buffer) {
        const CHAR_SIZE: u16 = 2;

        let vertex = [area.x, area.y, area.x + area.width - 1, area.y + area.height - 1];
        let lt = (vertex[0], vertex[1]);
        let rt = (vertex[2], vertex[1]);
        let lb = (vertex[0], vertex[3]);
        let rb = (vertex[2], vertex[3]);

        use MessageBorderAnimationPos::*;
        let pos = match self.animation.pos {
            Top(n) => if n + CHAR_SIZE >= area.width - 2 { Right(0) } else { Top(n + CHAR_SIZE) }
            Bottom(n) => if n + CHAR_SIZE >= area.width - 2 { Left(0) } else { Bottom(n + CHAR_SIZE) }
            Left(n) => if n + CHAR_SIZE >= area.height - 2 { Top(0) } else { Left(n + CHAR_SIZE) }
            Right(n) => if n + CHAR_SIZE >= area.height - 2 { Bottom(0) } else { Right(n + CHAR_SIZE) }
        };

        let ani_char = match pos {
            Top(_) => ANIMATION_CHAR_TOP,
            Bottom(_) => ANIMATION_CHAR_BOTTOM,
            Left(_) => ANIMATION_CHAR_LEFT,
            Right(_) => ANIMATION_CHAR_RIGHT,
        };

        let render_pos: [(u16, u16); CHAR_SIZE as usize] = match pos {
            Top(n) => [(lt.0 + n, lt.1), (lt.0 + n + 1, lt.1)],
            Bottom(n) => [(rb.0 - n, rb.1), (rb.0 - n + 1, rb.1)],
            Left(n) => [(lb.0, lb.1 - n), (lb.0, lb.1 - n + 1)],
            Right(n) => [(rt.0, rt.1 + n), (rt.0, rt.1 + n + 1)],
        };

        render_pos.iter().for_each(|(x, y)| {
            buf.cell_mut((*x, *y)).map(|cell| cell.set_char(ani_char));
        });

        self.animation.pos = pos;
        self.animation.frame = match self.animation.frame {
            u8::MAX => 0,
            i => i + 1,
        };
    }
}

pub struct Message {
    pub streaming: bool,
}

impl Message {
    fn render_border(&self, area: Rect, buf: &mut Buffer, streaming: bool, state: &mut MessageState) {
        let color = if streaming { Color::Green } else { Color::White };
        let mut block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(color));

        if streaming {
            let mut title = "Generating".to_string();
            match state.animation.frame % 3 {
                0 => title.push_str("."),
                1 => title.push_str(".."),
                2 => title.push_str("..."),
                _ => {}
            };
            block = block.title(title);
        } else {
            block = block.title("Chat");
        }

        block.render(area, buf);
    }
}

impl StatefulWidgetRef for Message {
    type State = MessageState;

    fn render_ref(&self, area:Rect, buf: &mut Buffer, state: &mut Self::State) {
        self.render_border(area, buf, self.streaming, state);
        if self.streaming {
            state.render_border_animation(area, buf);
        }

        let area = Rect::new(area.x + 1, area.y + 1, area.width - 2, area.height - 2);
        state.draw(area, buf);
    }
}

impl StatefulWidget for Message {
    type State = MessageState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        self.render_ref(area, buf, state);
    }
}

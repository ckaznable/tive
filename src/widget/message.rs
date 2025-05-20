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

#[derive(Debug, Clone)]
pub struct MessageState {
    buf: Buffer,
    scroll_y: u16,
    viewport: Rect,
}

impl MessageState {
    pub fn new(viewport: Rect) -> Self {
        Self {
            buf: Buffer::default(),
            scroll_y: 0,
            viewport,
        }
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
}

pub struct Message;

impl StatefulWidgetRef for Message {
    type State = MessageState;

    fn render_ref(&self, area:Rect, buf: &mut Buffer, state: &mut Self::State) {
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::White))
            .title("Chat")
            .render(area, buf);

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

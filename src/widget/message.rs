use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    widgets::{
        Paragraph, StatefulWidget, StatefulWidgetRef, Widget
    }
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
        let opt = textwrap::Options::new(self.viewport.width as usize).word_separator(textwrap::WordSeparator::AsciiSpace);
        let user_lines = textwrap::wrap(&user_message.body.content, &opt);
        let ai_lines = textwrap::wrap(&ai_message.body.content, opt);
        let total_lines = user_lines.len() + ai_lines.len();

        let rect = if total_lines >= self.viewport.height as usize {
            Rect::new(self.viewport.x, self.viewport.y, self.viewport.width, total_lines as u16)
        } else {
            self.viewport
        };

        self.buf.resize(rect);
        let [user_area, ai_area] = Layout::vertical([
            Constraint::Length(user_lines.len() as u16),
            Constraint::Min(0),
        ])
        .areas(rect);

        let user_paragraph = Paragraph::new(user_lines.join("\n"));
        user_paragraph.render(user_area, &mut self.buf);

        let ai_paragraph = Paragraph::new(ai_lines.join("\n"));
        ai_paragraph.render(ai_area, &mut self.buf);
    }
}

pub struct Message;

impl StatefulWidgetRef for Message {
    type State = MessageState;

    fn render_ref(&self, area:Rect, buf: &mut Buffer, state: &mut Self::State) {
        state.draw(area, buf);
    }
}

impl StatefulWidget for Message {
    type State = MessageState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        self.render_ref(area, buf, state);
    }
}

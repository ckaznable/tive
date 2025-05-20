use ratatui::{
    buffer::Buffer, layout::Rect, widgets::{
        Block, Borders, Paragraph, Widget, WidgetRef, Wrap
    }
};

use crate::tui::InputMode;

pub struct StatusBar {
    pub mode: InputMode,
}

impl StatusBar {
    #[inline]
    pub fn content(&self) -> &str {
        match self.mode {
            InputMode::Normal => "[q] quit | [i, a] chat",
            InputMode::Insert => "[esc] normal | [enter] send",
            InputMode::Leader => "[esc] normal | [e] edit file",
            InputMode::EditFile => "[esc] normal | [m] edit model config | [s] edit mcp config",
        }
    }
}

impl WidgetRef for StatusBar {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::TOP);

        Paragraph::new(self.content())
            .block(block)
            .wrap(Wrap { trim: true })
            .render(area, buf);
    }
}

impl Widget for StatusBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.render_ref(area, buf);
    }
}

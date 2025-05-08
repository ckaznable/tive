use ratatui::{
    buffer::Buffer, layout::Rect, widgets::{
        Block, Borders, Paragraph, Widget, WidgetRef, Wrap
    }
};

use crate::tui::InputMode;

pub struct StatusBar {
    pub mode: InputMode,
}

impl WidgetRef for StatusBar {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::TOP);

        Paragraph::new("[q] quit | [i, a] chat")
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

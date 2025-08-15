use crossterm::{cursor, queue, style, terminal};
use reqwest::Url;
use std::io::{self, Stdout, stdout};

use element::*;
use parsing::*;

mod element;
mod parsing;

#[expect(dead_code)]
#[derive(Default)]
struct Webpage {
    title: Option<String>,
    url: Option<Url>,
    root: Option<Element>,
}
#[derive(Clone, Copy, PartialEq)]
enum TextAlignment {
    Left,
    Centre,
    Right,
}
#[derive(Clone, Copy, PartialEq)]
struct ElementDrawContext {
    text_align: Option<TextAlignment>,
    foreground_color: Option<style::Color>,
    bold: bool,
    italics: bool,
    respect_whitespace: bool,
}
impl ElementDrawContext {
    fn merge(&mut self, other: &ElementDrawContext) {
        self.text_align = other.text_align.or(self.text_align);
        self.foreground_color = other.foreground_color.or(self.foreground_color);
        self.bold |= other.bold;
        self.italics |= other.italics;
        self.respect_whitespace |= other.respect_whitespace;
    }
}
#[derive(Clone)]
struct GlobalDrawContext<'a> {
    x: u16,
    y: u16,
    actual_cursor_x: u16,
    actual_cursor_y: u16,
    on_newline: bool,
    stdout: &'a Stdout,
    last_draw_ctx: ElementDrawContext,
}
struct Toad {
    tabs: Vec<Webpage>,
    tab_index: usize,
}
impl Toad {
    fn draw(&self) -> io::Result<()> {
        let stdout = stdout();
        self.clear_screen(&stdout)?;
        self.draw_current_page(&stdout, 0, 2)
    }
    fn clear_screen(&self, mut stdout: &Stdout) -> io::Result<()> {
        queue!(
            stdout,
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0)
        )
    }
    fn draw_current_page(&self, stdout: &Stdout, x: u16, y: u16) -> io::Result<()> {
        let Some(tab) = self.tabs.get(self.tab_index) else {
            return Ok(());
        };
        let mut ctx = GlobalDrawContext {
            x,
            y,
            actual_cursor_x: 0,
            actual_cursor_y: 0,
            on_newline: true,
            stdout,
            last_draw_ctx: DEFAULT_DRAW_CTX,
        };
        tab.root.as_ref().unwrap().draw(DEFAULT_DRAW_CTX, &mut ctx)
    }
}
#[cfg(test)]
mod tests {
    use crate::parsing::parse_html;

    #[test]
    fn print_test() {
        let test_page = parse_html(include_str!("test.html")).unwrap();
        println!("{:?}", test_page.root);
    }
}
fn main() -> io::Result<()> {
    let test_page = parse_html(include_str!("test.html")).unwrap();
    let toad = Toad {
        tabs: vec![test_page],
        tab_index: 0,
    };
    toad.draw()
}

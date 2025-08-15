use crossterm::{cursor, queue, style, terminal};
use reqwest::Url;
use std::io::{self, Stdout, Write, stdout};

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
#[expect(dead_code)]
#[derive(Clone, Copy, PartialEq)]
enum TextAlignment {
    Left,
    Centre,
    Right,
}
#[derive(Clone, Copy, PartialEq)]
enum Display {
    Inline,
    Block,
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
#[expect(dead_code)]
#[derive(Clone)]
struct GlobalDrawContext<'a> {
    width: u16,
    height: u16,
    x: u16,
    y: u16,
    actual_cursor_x: u16,
    actual_cursor_y: u16,
    stdout: &'a Stdout,
    last_draw_ctx: ElementDrawContext,
}
impl GlobalDrawContext<'_> {
    fn draw_line(&mut self, text: &str, draw_ctx: ElementDrawContext) -> io::Result<()> {
        let text_len = text.len() as u16;
        let offset_x = match draw_ctx.text_align {
            Some(TextAlignment::Centre) => (self.width - self.x) / 2 - text_len / 2,
            Some(TextAlignment::Right) => self.width - text_len,
            _ => 0,
        };
        self.x += offset_x;

        if self.x != self.actual_cursor_x || self.y != self.actual_cursor_y {
            queue!(self.stdout, cursor::MoveTo(self.x, self.y))?
        }
        apply_draw_ctx(draw_ctx, &mut self.last_draw_ctx, self.stdout)?;
        self.stdout.lock().write_all(text.as_bytes())?;
        self.x += text_len;
        self.actual_cursor_x = self.x;
        self.actual_cursor_y = self.y;

        Ok(())
    }
    fn draw_text(&mut self, text: &str, draw_ctx: ElementDrawContext) -> io::Result<()> {
        let start_x = self.x;
        let mut lines = text.lines().peekable();
        while let Some(line) = lines.next() {
            self.draw_line(line, draw_ctx)?;
            if lines.peek().is_some() {
                self.x = start_x;
                self.y += 1;
            }
        }
        Ok(())
    }
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
        let (width, height) = terminal::size()?;
        let mut ctx = GlobalDrawContext {
            width,
            height,
            x,
            y,
            actual_cursor_x: 0,
            actual_cursor_y: 0,
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

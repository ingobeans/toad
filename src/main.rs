use crossterm::{cursor, event, execute, queue, style, terminal};
use reqwest::Url;
use std::io::{self, stdout, Stdout, Write};

use element::*;
use parsing::*;

mod css;
mod element;
mod parsing;

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
enum Display {
    Inline,
    Block,
    None,
}
#[derive(Clone, Copy, PartialEq)]
struct ElementDrawContext {
    text_align: Option<TextAlignment>,
    foreground_color: Option<style::Color>,
    background_color: Option<style::Color>,
    display: Option<Display>,
    bold: bool,
    italics: bool,
    respect_whitespace: bool,
}
static DEFAULT_DRAW_CTX: ElementDrawContext = ElementDrawContext {
    text_align: None,
    foreground_color: None,
    background_color: None,
    display: None,
    bold: false,
    italics: false,
    respect_whitespace: false,
};
impl ElementDrawContext {
    fn merge(&mut self, other: &ElementDrawContext) {
        self.text_align = other.text_align.or(self.text_align);
        self.foreground_color = other.foreground_color.or(self.foreground_color);
        self.background_color = other.background_color.or(self.background_color);
        self.bold |= other.bold;
        self.italics |= other.italics;
        self.respect_whitespace |= other.respect_whitespace;
        // dont merge display, since it isnt inherited
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
    fn run(&mut self) -> io::Result<()> {
        let mut running = true;
        self.draw()?;
        while running {
            let event = event::read()?;
            if !event.is_key_press() {
                continue;
            }
            let event::Event::Key(key) = event else {
                continue;
            };
            match key.code {
                event::KeyCode::Enter => {
                    self.draw()?;
                }
                event::KeyCode::Tab => {
                    self.tab_index += 1;
                    if self.tab_index >= self.tabs.len() {
                        self.tab_index = 0;
                    }
                    self.draw()?;
                }
                event::KeyCode::Char(char) => {
                    if char == 'q' {
                        running = false;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn draw(&self) -> io::Result<()> {
        let stdout = stdout();
        self.clear_screen(&stdout)?;
        self.draw_topbar(&stdout)?;
        self.draw_current_page(&stdout)
    }
    fn draw_topbar(&self, mut stdout: &Stdout) -> io::Result<()> {
        queue!(
            stdout,
            style::SetBackgroundColor(style::Color::Grey),
            style::SetForegroundColor(style::Color::Black),
            terminal::Clear(terminal::ClearType::CurrentLine),
        )?;
        for (index, tab) in self.tabs.iter().enumerate() {
            let text = if let Some(title) = &tab.title {
                title.trim()
            } else if let Some(url) = &tab.url {
                &url.to_string()
            } else {
                "untitled"
            };
            if index == self.tab_index {
                queue!(stdout, style::SetBackgroundColor(style::Color::White))?;
                print!("[{text}]");
                queue!(stdout, style::SetBackgroundColor(style::Color::Grey))?;
                print!(" ")
            } else {
                print!("[{text}] ");
            }
        }
        queue!(stdout, style::ResetColor)?;
        Ok(())
    }
    fn clear_screen(&self, mut stdout: &Stdout) -> io::Result<()> {
        queue!(
            stdout,
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0)
        )
    }
    fn draw_current_page(&self, mut stdout: &Stdout) -> io::Result<()> {
        let Some(tab) = self.tabs.get(self.tab_index) else {
            return Ok(());
        };
        let x = 0;
        let y = 1;
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
        tab.root
            .as_ref()
            .unwrap()
            .draw(DEFAULT_DRAW_CTX, &mut ctx)?;
        execute!(stdout, style::ResetColor)
    }
}
fn main() -> io::Result<()> {
    let mut toad = Toad {
        tabs: vec![
            parse_html(include_str!("home.html")).unwrap(),
            parse_html(include_str!("test.html")).unwrap(),
        ],
        tab_index: 0,
    };
    toad.run()
}

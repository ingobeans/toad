use crossterm::{cursor, event, execute, queue, style, terminal};
use reqwest::{Client, Url};
use std::{
    collections::HashMap,
    io::{self, stdout, Stdout, Write},
    str::FromStr,
};

use consts::*;
use element::*;
use parsing::*;

mod consts;
mod css;
mod element;
mod parsing;
mod utils;

#[derive(Default)]
struct Webpage {
    title: Option<String>,
    url: Option<Url>,
    root: Option<Element>,
    global_style: HashMap<StyleTarget, ElementDrawContext>,
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
enum Measurement {
    FitContentWidth,
    FitContentHeight,
    PercentWidth(f32),
    PercentHeight(f32),
    Pixels(u16),
}
impl Measurement {
    fn to_pixels(
        self,
        screen_size: (u16, u16),
        element: &Element,
        global_ctx: &GlobalDrawContext,
    ) -> u16 {
        match &self {
            Self::Pixels(pixels) => *pixels,
            Self::PercentHeight(percent) => ((screen_size.1 as f32 * percent) * LH as f32) as u16,
            Self::PercentWidth(percent) => ((screen_size.0 as f32 * percent) * EM as f32) as u16,
            Self::FitContentWidth => {
                let mut width = 0;
                for child in &element.children {
                    let cw = if child.ty.name == "node" {
                        child.text.as_ref().map(|f| f.len()).unwrap_or(0) as u16 * EM
                    } else if let Some(w) = child.get_active_style(global_ctx).width {
                        w.to_pixels(screen_size, element, global_ctx)
                    } else {
                        continue;
                    };
                    if cw > width {
                        width = cw;
                    }
                }
                width
            }
            Self::FitContentHeight => {
                let mut height = 0;
                for child in &element.children {
                    let cw = if child.ty.name == "node" {
                        LH
                    } else if let Some(w) = child.get_active_style(global_ctx).height {
                        w.to_pixels(screen_size, element, global_ctx)
                    } else {
                        continue;
                    };
                    if cw > height {
                        height = cw;
                    }
                }
                height
            }
        }
    }
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
    width: Option<Measurement>,
    height: Option<Measurement>,
}
static DEFAULT_DRAW_CTX: ElementDrawContext = ElementDrawContext {
    text_align: None,
    foreground_color: None,
    background_color: None,
    display: None,
    bold: false,
    italics: false,
    respect_whitespace: false,
    width: None,
    height: None,
};
impl ElementDrawContext {
    fn merge_all(&mut self, other: &ElementDrawContext) {
        self.merge(other);
        self.display = other.display.or(self.display);
        self.width = other.width.or(self.width);
        self.height = other.height.or(self.height);
    }
    fn merge(&mut self, other: &ElementDrawContext) {
        self.text_align = other.text_align.or(self.text_align);
        self.foreground_color = other.foreground_color.or(self.foreground_color);
        self.background_color = other.background_color.or(self.background_color);
        self.bold |= other.bold;
        self.italics |= other.italics;
        self.respect_whitespace |= other.respect_whitespace;
        // dont merge properties like display, width or height since they arent inherited
    }
}

#[derive(Clone, Hash, PartialEq, Eq, Debug)]
enum StyleTarget {
    ElementType(String),
    Class(String),
    Id(String),
}
impl StyleTarget {
    fn matches(&self, element: &Element) -> bool {
        match self {
            StyleTarget::ElementType(ty) => element.ty.name == ty,
            StyleTarget::Class(class) => element.classes.contains(class),
            StyleTarget::Id(id) => element.get_attribute("id").is_some_and(|i| i == id),
        }
    }
}

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
    global_style: &'a HashMap<StyleTarget, ElementDrawContext>,
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
    client: Client,
}
impl Toad {
    fn new(tabs: Vec<Webpage>) -> Result<Self, reqwest::Error> {
        let client = Client::builder()
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:141.0) Gecko/20100101 Firefox/141.0",
            )
            .build()?;
        Ok(Self {
            tabs,
            tab_index: 0,
            client,
        })
    }
    async fn get_url(&self, url: Url) -> Option<Webpage> {
        let response = self.client.get(url.clone()).send().await.ok()?;
        let data = response.text().await.ok()?;
        let page = parse_html(&data);
        page.map(|mut f| {
            f.url = Some(url);
            f
        })
    }
    async fn run(&mut self) -> io::Result<()> {
        let mut running = true;
        let mut stdout = stdout();
        self.draw(&stdout)?;
        terminal::enable_raw_mode()?;
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
                    self.draw(&stdout)?;
                }
                event::KeyCode::Tab => {
                    self.tab_index += 1;
                    if self.tab_index >= self.tabs.len() {
                        self.tab_index = 0;
                    }
                    self.draw(&stdout)?;
                }
                event::KeyCode::Char(char) => {
                    if char == 'q' {
                        running = false;
                    }
                    if char == 'g' {
                        terminal::disable_raw_mode()?;
                        execute!(
                            stdout,
                            cursor::MoveTo(0, 1),
                            terminal::Clear(terminal::ClearType::CurrentLine),
                            cursor::Show
                        )?;
                        let mut buf = String::new();
                        io::stdin().read_line(&mut buf)?;
                        terminal::enable_raw_mode()?;
                        if let Ok(url) = Url::from_str(&buf) {
                            if let Some(page) = self.get_url(url).await {
                                self.tab_index += 1;
                                self.tabs.insert(self.tab_index, page);
                            }
                        }

                        self.draw(&stdout)?;
                    }
                }
                _ => {}
            }
        }
        terminal::disable_raw_mode()?;
        execute!(stdout, cursor::Show)?;
        Ok(())
    }
    fn draw(&self, stdout: &Stdout) -> io::Result<()> {
        self.clear_screen(stdout)?;
        self.draw_topbar(stdout)?;
        self.draw_current_page(stdout)
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
        queue!(stdout, cursor::MoveToNextLine(1))?;
        queue!(stdout, terminal::Clear(terminal::ClearType::CurrentLine))?;
        if let Some(Some(url)) = self.tabs.get(self.tab_index).map(|f| &f.url) {
            print!("{url}")
        }
        queue!(stdout, style::ResetColor)?;
        Ok(())
    }
    fn clear_screen(&self, mut stdout: &Stdout) -> io::Result<()> {
        queue!(
            stdout,
            terminal::Clear(terminal::ClearType::Purge),
            cursor::MoveTo(0, 0),
            cursor::Hide,
        )
    }
    fn draw_current_page(&self, mut stdout: &Stdout) -> io::Result<()> {
        let Some(tab) = self.tabs.get(self.tab_index) else {
            return Ok(());
        };
        let x = 0;
        let y = 2;
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
            global_style: &tab.global_style,
        };
        tab.root
            .as_ref()
            .unwrap()
            .draw(DEFAULT_DRAW_CTX, &mut ctx)?;
        execute!(stdout, style::ResetColor)
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut toad = Toad::new(vec![
        parse_html(include_str!("home.html")).unwrap(),
        parse_html(include_str!("test.html")).unwrap(),
    ])
    .unwrap();
    toad.run().await
}

use crossterm::{cursor, event, execute, queue, style, terminal};
use reqwest::{Client, Url};
use std::{
    collections::HashMap,
    fmt::Debug,
    io::{self, Stdout, Write, stdout},
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
    scroll_y: u16,
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
#[derive(Clone, Copy, Debug, PartialEq)]
enum ActualMeasurement {
    Pixels(u16),
    PercentOfUnknown(usize, f32),
    Waiting(usize),
}
impl ActualMeasurement {
    fn get_pixels(self) -> Option<u16> {
        match self {
            Self::Pixels(p) => Some(p),
            _ => None,
        }
    }
    fn get_pixels_lossy(self) -> u16 {
        match self {
            Self::Pixels(p) => p,
            _ => 0,
        }
    }
}
impl Default for ActualMeasurement {
    fn default() -> Self {
        Self::Waiting(999)
    }
}
#[derive(Clone, Copy, PartialEq, Debug)]
enum Measurement {
    FitContentWidth,
    FitContentHeight,
    PercentWidth(f32),
    PercentHeight(f32),
    Pixels(u16),
}

#[derive(Clone, Copy, PartialEq, Debug)]
/// For CSS properties that are not inherited by default. They can either be unset, forced to inherit, or have a specified value.
/// This is the alternative to the other CSS fields which are only represented by an [Option], as they are either unset or specfified, and automatically inherit when unset.
enum NonInheritedField<T> {
    Unset,
    Inherit,
    Specified(T),
}
impl<T> NonInheritedField<T> {
    fn inherit_from(&mut self, b: Self) {
        if let Inherit = self {
            *self = b;
        }
    }
    fn unwrap_or(self, other: T) -> T {
        match self {
            Specified(v) => v,
            _ => other,
        }
    }
    fn set_or(self, other: Self) -> Self {
        match &self {
            Unset => other,
            _ => self,
        }
    }
}
use NonInheritedField::*;

#[derive(Clone, Copy, PartialEq)]
struct ElementDrawContext {
    text_align: Option<TextAlignment>,
    foreground_color: Option<style::Color>,
    background_color: NonInheritedField<style::Color>,
    display: NonInheritedField<Display>,
    bold: bool,
    italics: bool,
    respect_whitespace: bool,
    width: Option<Measurement>,
    height: NonInheritedField<Measurement>,
}
static DEFAULT_DRAW_CTX: ElementDrawContext = ElementDrawContext {
    text_align: None,
    foreground_color: None,
    background_color: Unset,
    display: Unset,
    bold: false,
    italics: false,
    respect_whitespace: false,
    width: None,
    height: Unset,
};
impl ElementDrawContext {
    /// Merges this context with another, exclusively copying inherited fields
    fn merge_inherit(&mut self, other: &ElementDrawContext) {
        self.text_align = other.text_align.or(self.text_align);
        self.foreground_color = other.foreground_color.or(self.foreground_color);
        self.width = other.width.or(self.width);
        self.bold |= other.bold;
        self.italics |= other.italics;
        self.respect_whitespace |= other.respect_whitespace;
    }
    /// Merges this context with another, copying all unset fields
    fn merge_all(&mut self, other: &ElementDrawContext) {
        self.merge_inherit(other);
        self.display = other.display.set_or(self.display);
        self.height = other.height.set_or(self.height);
        self.background_color = other.background_color.set_or(self.background_color);
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

#[derive(PartialEq, Clone)]
enum DrawCall {
    /// X, Y, W, H, Color
    Rect(u16, u16, ActualMeasurement, ActualMeasurement, style::Color),
    /// X, Y, Text,  DrawContext
    Text(u16, u16, String, ElementDrawContext, ActualMeasurement),
}
impl DrawCall {
    fn order(&self) -> u8 {
        match self {
            DrawCall::Rect(_, _, _, _, _) => 0,
            DrawCall::Text(_, _, _, _, _) => 1,
        }
    }
}
impl Debug for DrawCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DrawCall::Rect(x, y, w, h, c) => {
                f.write_str(&format!("Rect({x},{y},{w:?},{h:?},{c:?})"))
            }
            DrawCall::Text(x, y, text, _, _) => f.write_str(&format!("Text({x},{y},'{text}')")),
        }
    }
}
struct GlobalDrawContext<'a> {
    /// The global CSS stylesheet
    global_style: &'a HashMap<StyleTarget, ElementDrawContext>,
    /// Buffer that all elements with unknown sizes are added to, such that any relative size to an unknown can later be evaluated.
    unknown_sized_elements: Vec<Option<ActualMeasurement>>,
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
                event::KeyCode::Down => {
                    if let Some(tab) = self.tabs.get_mut(self.tab_index) {
                        tab.scroll_y += 1;
                        self.draw(&stdout)?;
                    }
                }
                event::KeyCode::Up => {
                    if let Some(tab) = self.tabs.get_mut(self.tab_index) {
                        tab.scroll_y = tab.scroll_y.saturating_sub(1);
                        self.draw(&stdout)?;
                    }
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
                        if let Ok(url) = Url::from_str(&buf)
                            && let Some(page) = self.get_url(url).await
                        {
                            self.tab_index += 1;
                            self.tabs.insert(self.tab_index, page);
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
    fn draw(&self, mut stdout: &Stdout) -> io::Result<()> {
        self.clear_screen(stdout)?;
        self.draw_current_page(stdout)?;
        self.draw_topbar(stdout)?;
        stdout.flush()?;
        Ok(())
    }
    fn draw_topbar(&self, mut stdout: &Stdout) -> io::Result<()> {
        queue!(
            stdout,
            cursor::MoveTo(0, 0),
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
        let start_x = 0;
        let start_y = 2;
        let (screen_width, screen_height) = terminal::size()?;
        let screen_height = screen_height - start_y;
        let mut global_ctx = GlobalDrawContext {
            unknown_sized_elements: Vec::new(),
            global_style: &tab.global_style,
        };
        let mut draw_data = DrawData {
            parent_width: ActualMeasurement::Pixels(screen_width * EM),
            parent_height: ActualMeasurement::Pixels(screen_height * LH),
            ..Default::default()
        };
        queue!(
            stdout,
            cursor::MoveTo(start_x, start_y),
            terminal::Clear(terminal::ClearType::FromCursorDown)
        )?;
        tab.root
            .as_ref()
            .unwrap()
            .draw(DEFAULT_DRAW_CTX, &mut global_ctx, &mut draw_data)?;

        // sort draw calls such that rect calls are drawn first
        draw_data.draw_calls.sort_by_key(|a| a.order());
        // reverse because vecs are LIFO
        draw_data.draw_calls.reverse();
        let mut last = DEFAULT_DRAW_CTX;
        let mut rects: Vec<(u16, u16, u16, u16, style::Color)> = Vec::new();
        let mut actual_cursor_x = 0;
        let mut actual_cursor_y = 0;

        fn actualize_actual(
            a: ActualMeasurement,
            unknown_sized_elements: &Vec<Option<ActualMeasurement>>,
        ) -> u16 {
            match a {
                ActualMeasurement::Pixels(p) => p,
                ActualMeasurement::PercentOfUnknown(i, p) => {
                    (actualize_actual(unknown_sized_elements[i].unwrap(), unknown_sized_elements)
                        as f32
                        * p) as u16
                }
                ActualMeasurement::Waiting(_) => 0,
            }
        }
        while let Some(call) = draw_data.draw_calls.pop() {
            match call {
                DrawCall::Rect(x, y, w, h, color) => {
                    let x = x / EM + start_x;
                    let mut y = y / LH + start_y;

                    let w = actualize_actual(w, &global_ctx.unknown_sized_elements);
                    let h = actualize_actual(h, &global_ctx.unknown_sized_elements);
                    let w = w / EM;
                    let mut h = h / LH;
                    if x == start_x
                        && y == start_y
                        && w + start_x >= screen_width
                        && h + start_y >= screen_height
                    {
                        if x != actual_cursor_x || y != actual_cursor_y {
                            actual_cursor_x = x;
                            actual_cursor_y = y;
                            queue!(stdout, cursor::MoveTo(x, y))?;
                        }
                        queue!(
                            stdout,
                            style::SetBackgroundColor(color),
                            terminal::Clear(terminal::ClearType::FromCursorDown)
                        )?;
                        continue;
                    }
                    let bottom_out = y - start_y < tab.scroll_y;

                    if bottom_out && y + h - start_y < tab.scroll_y {
                        continue;
                    } else if bottom_out {
                        let o = y;
                        y = start_y + tab.scroll_y;
                        h -= y - o;
                    } else if y - tab.scroll_y > (screen_height + start_y) {
                        continue;
                    } else if y + h - tab.scroll_y > (screen_height + start_y) {
                        h = (screen_height + start_y) + tab.scroll_y - y;
                    }

                    let y = y.saturating_sub(tab.scroll_y);
                    rects.push((x, y, w, h, color));

                    let mut ctx = last;
                    ctx.background_color = Specified(color);
                    apply_draw_ctx(ctx, &mut last, &mut stdout.lock())?;
                    for i in 0..h {
                        queue!(stdout, cursor::MoveTo(x, y + i))?;
                        stdout.lock().write_all(&b" ".repeat(w as _))?;
                        actual_cursor_x = x + w;
                        actual_cursor_y = y + h;
                    }
                }
                DrawCall::Text(x, y, text, ctx, parent_width) => {
                    apply_draw_ctx(ctx, &mut last, &mut stdout.lock())?;
                    let width =
                        actualize_actual(parent_width, &global_ctx.unknown_sized_elements) / EM;
                    for (index, line) in text.lines().enumerate() {
                        let text_len = line.len() as u16;
                        let x = x / EM + start_x;
                        let offset_x = match ctx.text_align {
                            Some(TextAlignment::Centre) => (width - x) / 2 - text_len / 2,
                            Some(TextAlignment::Right) => width - text_len,
                            _ => 0,
                        };
                        let x = x + offset_x;
                        let y = y / LH + index as u16 + start_y;
                        if y - start_y < tab.scroll_y
                            || y - tab.scroll_y >= (screen_height + start_y)
                        {
                            continue;
                        }
                        let y = y - tab.scroll_y;
                        let mut chunks = Vec::new();
                        if let Unset = ctx.background_color {
                            // check if its over any rects
                            for (rx, ry, rw, rh, color) in rects.iter() {
                                let horizontal_range = *rx..(rx + rw);
                                let vertical_range = *ry..(ry + rh);
                                if vertical_range.contains(&y)
                                    && (horizontal_range.contains(&x)
                                        || horizontal_range.contains(&(x + text_len)))
                                {
                                    let start = rx.max(&x) - x;
                                    let end = (rx + rw).min(x + text_len) - x;
                                    // remove any chunks that are covered by this chunk
                                    chunks.retain(|(s, e, _)| *s < start || *e > end);

                                    chunks.push((start, end, color));
                                }
                            }
                        }
                        if x != actual_cursor_x || y != actual_cursor_y {
                            queue!(stdout, cursor::MoveTo(x, y))?;
                        }
                        actual_cursor_y = y;
                        actual_cursor_x = x;
                        if chunks
                            .first()
                            .is_none_or(|(start, end, _)| *start > 0 || *end < line.len() as u16)
                        {
                            print!("{line}");
                            actual_cursor_x = x + text_len;
                        }
                        for (start, end, color) in chunks.into_iter() {
                            let x = start + x;
                            let line = &line[start as usize..end as usize];
                            let mut ctx = ctx;
                            ctx.background_color = Specified(*color);
                            apply_draw_ctx(ctx, &mut last, &mut stdout.lock())?;
                            if x != actual_cursor_x {
                                actual_cursor_x = x + line.len() as u16;
                                queue!(stdout, cursor::MoveToColumn(x))?;
                            }
                            print!("{line}");
                        }
                    }
                }
            }
        }

        queue!(stdout, style::ResetColor)
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

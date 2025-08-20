use crossterm::{cursor, event, execute, queue, style, terminal};
use reqwest::{Client, Url};
use std::{
    collections::HashMap,
    fmt::Debug,
    io::{self, Stdout, Write, stdout},
    str::FromStr,
    time::Duration,
};
use tokio::task::JoinHandle;

use consts::*;
use element::*;
use parsing::*;
use utils::*;

mod consts;
mod css;
mod element;
mod parsing;
mod utils;

#[derive(Clone)]
struct CachedDraw {
    calls: Vec<DrawCall>,
    unknown_sized_elements: Vec<Option<ActualMeasurement>>,
    content_height: u16,
}

#[derive(Default)]
struct Webpage {
    indentifier: usize,
    title: Option<String>,
    url: Option<Url>,
    root: Option<Element>,
    global_style: Vec<(StyleTarget, ElementDrawContext)>,
    scroll_y: u16,
    /// Which interactable element we're tabbed to
    tab_index: Option<usize>,
    /// Each draw, update this with whatever interactable element the tab_index points to
    hovered_interactable: Option<InteractableElement>,
    debug_info: WebpageDebugInfo,
    cached_draw: Option<CachedDraw>,
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

use crate::css::parse_stylesheet;

#[derive(Clone, Copy, PartialEq)]
enum TextPrefix {
    Dot,
    Number,
}

#[derive(Clone, Copy, PartialEq)]
struct ElementDrawContext {
    text_align: Option<TextAlignment>,
    foreground_color: Option<style::Color>,
    background_color: NonInheritedField<style::Color>,
    display: NonInheritedField<Display>,
    bold: bool,
    italics: bool,
    respect_whitespace: bool,
    width: NonInheritedField<Measurement>,
    height: NonInheritedField<Measurement>,
    text_prefix: Option<TextPrefix>,
}
static DEFAULT_DRAW_CTX: ElementDrawContext = ElementDrawContext {
    text_align: None,
    foreground_color: None,
    background_color: Unset,
    display: Unset,
    bold: false,
    italics: false,
    respect_whitespace: false,
    width: Unset,
    height: Unset,
    text_prefix: None,
};
impl ElementDrawContext {
    /// Merges this context with another, exclusively copying inherited fields
    fn merge_inherit(&mut self, other: &ElementDrawContext) {
        self.text_align = other.text_align.or(self.text_align);
        self.foreground_color = other.foreground_color.or(self.foreground_color);
        self.text_prefix = other.text_prefix.or(self.text_prefix);
        self.bold |= other.bold;
        self.italics |= other.italics;
        self.respect_whitespace |= other.respect_whitespace;
    }
    /// Merges this context with another, copying all unset fields
    fn merge_all(&mut self, other: &ElementDrawContext) {
        self.merge_inherit(other);
        self.display = other.display.set_or(self.display);
        self.height = other.height.set_or(self.height);
        self.width = other.width.set_or(self.width);
        self.background_color = other.background_color.set_or(self.background_color);
    }
}

#[derive(Clone, Hash, PartialEq, Eq, Debug)]
enum StyleTargetType {
    /// Target by element type (Name)
    ElementType(String),
    /// Target by element class (Class name, Optional element type requirement)
    Class(String, Option<String>),
    /// Target by element id (Id, Optional element type requirement)
    Id(String, Option<String>),
}

impl StyleTargetType {
    fn matches_one(&self, info: &ElementTargetInfo) -> bool {
        match self {
            StyleTargetType::ElementType(ty) => info.type_name == ty,
            StyleTargetType::Class(class, ty) => {
                info.classes.contains(class) && ty.as_ref().is_none_or(|ty| ty == info.type_name)
            }
            StyleTargetType::Id(id, ty) => {
                info.id.as_ref().is_some_and(|i| i == id)
                    && ty.as_ref().is_none_or(|ty| ty == info.type_name)
            }
        }
    }
}

#[derive(Clone, Hash, PartialEq, Eq, Debug)]
struct StyleTarget {
    types: Vec<StyleTargetType>,
}

#[derive(Clone)]
struct ElementTargetInfo {
    type_name: &'static str,
    id: Option<String>,
    classes: Vec<String>,
}
impl StyleTarget {
    fn matches(&self, info: &[ElementTargetInfo]) -> bool {
        let mut info = info.iter().rev();
        let mut types = self.types.iter().rev();

        // unwrap because this function should never be called without passing at least the element self
        let first_element = info.next().unwrap();
        let Some(first_type) = types.next() else {
            return false;
        };
        if !first_type.matches_one(first_element) {
            return false;
        }

        'outer: for ty in types {
            for element in info.by_ref() {
                if ty.matches_one(element) {
                    continue 'outer;
                }
            }
            return false;
        }
        true
    }
}

fn refresh_style(page: &mut Webpage, assets: &HashMap<Url, DataEntry>) {
    let mut global_style = Vec::new();
    if let Some(root) = &page.root {
        let mut all_styles = String::new();
        get_all_styles(root, &mut all_styles, page.url.as_ref(), assets);
        parse_stylesheet(&all_styles, &mut global_style);
    }
    page.global_style = global_style;
}

#[derive(PartialEq, Clone)]
enum DrawCall {
    /// X, Y, W, H, Image Source Link
    Image(u16, u16, ActualMeasurement, ActualMeasurement, String),
    /// X, Y, W, H, Color
    Rect(u16, u16, ActualMeasurement, ActualMeasurement, style::Color),
    /// X, Y, Text, DrawContext, Parent Interactable
    Text(
        u16,
        u16,
        String,
        ElementDrawContext,
        ActualMeasurement,
        Option<InteractableElement>,
    ),
}
impl DrawCall {
    fn order(&self) -> u8 {
        match self {
            DrawCall::Rect(_, _, _, _, _) => 0,
            DrawCall::Image(_, _, _, _, _) => 1,
            DrawCall::Text(_, _, _, _, _, _) => 2,
        }
    }
}
impl Debug for DrawCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DrawCall::Image(x, y, w, h, source) => {
                f.write_str(&format!("Rect({x},{y},{w:?},{h:?},{source:?})"))
            }
            DrawCall::Rect(x, y, w, h, c) => {
                f.write_str(&format!("Rect({x},{y},{w:?},{h:?},{c:?})"))
            }
            DrawCall::Text(x, y, text, _, _, _) => f.write_str(&format!("Text({x},{y},'{text}')")),
        }
    }
}
#[derive(Clone, PartialEq)]
enum InteractableType {
    Link(String),
}
#[derive(Clone, PartialEq)]
struct InteractableElement {
    ty: InteractableType,
    index: usize,
}
struct GlobalDrawContext<'a> {
    /// The global CSS stylesheet
    global_style: &'a Vec<(StyleTarget, ElementDrawContext)>,
    /// Buffer that all elements with unknown sizes are added to, such that any relative size to an unknown can later be evaluated.
    unknown_sized_elements: Vec<Option<ActualMeasurement>>,
    /// Keeps track of current index for new interactables. Used so all interactables can have a unique ID
    interactable_index: usize,
}
#[derive(Clone, Debug)]
enum DataType {
    PlainText,
    Image,
}
enum DataEntry {
    PlainText(String),
    Image(image::DynamicImage),
}
// allow dead code because i sometimes want to use the info_log function for debugging
#[allow(dead_code)]
#[derive(Default, Clone)]
struct WebpageDebugInfo {
    info_log: Vec<String>,
    unknown_elements: Vec<String>,
    fetch_queue: Vec<(DataType, String)>,
}
impl Debug for WebpageDebugInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut log = String::new();
        for item in self.info_log.iter() {
            log += &format!("-{:?}\n", item);
        }
        write!(
            f,
            "Info Log: \n\n{log}\n\nUnknown elements: {:?}",
            self.unknown_elements
        )
    }
}

const DEBUG_PAGE: &str = include_str!("debug.html");

async fn get_data(url: Url, ty: DataType, client: Client) -> Option<DataEntry> {
    let resp = client.get(url).send().await.ok()?;
    match ty {
        DataType::Image => {
            let bytes: Vec<u8> = resp.bytes().await.ok().map(|f| f.into())?;
            let image = image::load_from_memory(&bytes).ok()?;
            Some(DataEntry::Image(image))
        }
        DataType::PlainText => {
            let text: String = resp.text().await.ok()?;
            Some(DataEntry::PlainText(text))
        }
    }
}

type FetchFuture = JoinHandle<Option<DataEntry>>;

#[derive(Default)]
struct Toad {
    tabs: Vec<Webpage>,
    tab_index: usize,
    client: Client,
    fetched_assets: HashMap<Url, DataEntry>,
    fetches: Vec<(usize, Url, FetchFuture)>,
    current_page_id: usize,
}
impl Toad {
    fn new() -> Result<Self, reqwest::Error> {
        let client = Client::builder()
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:141.0) Gecko/20100101 Firefox/141.0",
            )
            .build()?;
        Ok(Self {
            client,
            ..Default::default()
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
    fn open_page(&mut self, mut page: Webpage) {
        if !self.tabs.is_empty() {
            self.tab_index += 1;
        }
        refresh_style(&mut page, &self.fetched_assets);
        page.indentifier = self.current_page_id;
        for (ty, source) in page.debug_info.fetch_queue.drain(..) {
            let options = Url::options().base_url(page.url.as_ref());
            let Ok(url) = options.parse(&source) else {
                continue;
            };
            let handle = tokio::spawn(get_data(url.clone(), ty, self.client.clone()));
            self.fetches.push((page.indentifier, url, handle));
        }
        self.current_page_id += 1;
        self.tabs.insert(self.tab_index, page);
    }
    async fn run(&mut self) -> io::Result<()> {
        add_panic_handler();
        let mut running = true;
        let mut stdout = stdout();
        terminal::enable_raw_mode()?;
        queue!(stdout, cursor::Hide)?;
        self.draw_topbar(&stdout)?;
        self.draw(&stdout)?;
        while running {
            if event::poll(Duration::from_secs(1))? {
                let event = event::read()?;
                if !event.is_key_press() {
                    continue;
                }
                let event::Event::Key(key) = event else {
                    continue;
                };
                match key.code {
                    event::KeyCode::Enter => {
                        let Some(tab) = self.tabs.get(self.tab_index) else {
                            continue;
                        };
                        let Some(hovered) = &tab.hovered_interactable else {
                            continue;
                        };
                        match &hovered.ty {
                            InteractableType::Link(path) => {
                                let options = Url::options().base_url(tab.url.as_ref());
                                let Ok(url) = options.parse(path) else {
                                    continue;
                                };
                                if let Some(page) = self.get_url(url).await {
                                    self.open_page(page);
                                }

                                self.draw_topbar(&stdout)?;
                                self.draw(&stdout)?;
                            }
                        }
                    }
                    event::KeyCode::F(12) => {
                        if let Some(tab) = self.tabs.get(self.tab_index) {
                            let debug = tab.debug_info.clone();
                            let page_text = sanitize(
                                &tab.root
                                    .as_ref()
                                    .map(|f| f.print_recursive(0))
                                    .unwrap_or(String::new()),
                            );
                            let mut s = String::new();
                            for (item, _) in tab.global_style.iter() {
                                s += &format!("{:?}", item);
                                s += "\n\n"
                            }
                            let html = DEBUG_PAGE
                                .replace("{DEBUGINFO}", &sanitize(&format!("{:?}", debug)))
                                .replace("{STYLE_TARGETS}", &sanitize(&s))
                                .replace("{PAGE}", &page_text);
                            if let Some(page) = parse_html(&html) {
                                self.open_page(page);
                                self.draw_topbar(&stdout)?;
                                self.draw(&stdout)?;
                            }
                        }
                    }
                    event::KeyCode::Tab => {
                        self.tab_index += 1;
                        if self.tab_index >= self.tabs.len() {
                            self.tab_index = 0;
                        }
                        self.draw_topbar(&stdout)?;
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
                    event::KeyCode::Right => {
                        if let Some(tab) = self.tabs.get_mut(self.tab_index) {
                            tab.tab_index = Some(tab.tab_index.map(|i| i + 1).unwrap_or(0));
                            self.draw(&stdout)?;
                        }
                    }
                    event::KeyCode::Left => {
                        if let Some(tab) = self.tabs.get_mut(self.tab_index) {
                            tab.tab_index =
                                Some(tab.tab_index.map(|i| i.saturating_sub(1)).unwrap_or(0));
                            self.draw(&stdout)?;
                        }
                    }
                    event::KeyCode::PageDown => {
                        let (_, screen_height) = terminal::size()?;
                        if let Some(tab) = self.tabs.get_mut(self.tab_index) {
                            tab.scroll_y += screen_height;
                            self.draw(&stdout)?;
                        }
                    }
                    event::KeyCode::PageUp => {
                        let (_, screen_height) = terminal::size()?;
                        if let Some(tab) = self.tabs.get_mut(self.tab_index) {
                            tab.scroll_y = tab.scroll_y.saturating_sub(screen_height);
                            self.draw(&stdout)?;
                        }
                    }
                    event::KeyCode::Char(char) => {
                        let control = key.modifiers.contains(event::KeyModifiers::CONTROL);
                        if char == 'r' {
                            if let Some(page) = self.tabs.get_mut(self.tab_index) {
                                refresh_style(page, &self.fetched_assets);
                                page.cached_draw = None;
                            }
                            self.draw_topbar(&stdout)?;
                            self.draw(&stdout)?;
                        } else if char == 'q' {
                            running = false;
                        } else if char == 'w' && control {
                            if self.tab_index < self.tabs.len() {
                                self.tabs.remove(self.tab_index);
                                self.tab_index = self.tab_index.saturating_sub(1);
                                self.draw_topbar(&stdout)?;
                                self.draw(&stdout)?;
                            }
                        } else if char == 'g' {
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
                            queue!(stdout, cursor::Hide)?;
                            if let Ok(url) = Url::from_str(&buf)
                                && let Some(page) = self.get_url(url).await
                            {
                                self.open_page(page);
                                self.draw_topbar(&stdout)?;
                                self.draw(&stdout)?;
                            }
                        }
                    }
                    _ => {}
                }
            }
            // update fetch queue

            let mut any_changed = false;
            let mut death_queue = Vec::new();
            for (index, (page_id, url, handle)) in self.fetches.iter_mut().enumerate() {
                if handle.is_finished() {
                    let Ok(polled) = tokio::join!(handle).0 else {
                        continue;
                    };
                    death_queue.push(index);
                    let Some(data) = polled else {
                        if let Some(page) = self.tabs.iter_mut().find(|f| f.indentifier == *page_id)
                        {
                            page.debug_info
                                .info_log
                                .push(format!("Failed to get data of {url}"));
                        }
                        continue;
                    };
                    self.fetched_assets.insert(url.clone(), data);

                    // refresh page with this page_id
                    if let Some(page) = self.tabs.iter_mut().find(|f| f.indentifier == *page_id) {
                        refresh_style(page, &self.fetched_assets);
                        page.cached_draw = None;
                        any_changed = true;
                    }
                }
            }
            let mut index = 0;
            self.fetches.retain(|_| {
                let old = index;
                index += 1;
                !death_queue.contains(&old)
            });
            // if any finished loading
            if any_changed {
                self.draw_topbar(&stdout)?;
                self.draw(&stdout)?;
            }
        }
        terminal::disable_raw_mode()?;
        execute!(stdout, cursor::Show)?;
        Ok(())
    }
    fn draw(&mut self, mut stdout: &Stdout) -> io::Result<()> {
        self.draw_current_page(stdout)?;
        stdout.flush()
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
        stdout.flush()
    }
    fn draw_current_page(&mut self, mut stdout: &Stdout) -> io::Result<()> {
        let Some(tab) = self.tabs.get_mut(self.tab_index) else {
            return Ok(());
        };
        let start_x = 0;
        let start_y = 2;
        let (screen_width, screen_height) = terminal::size()?;
        let screen_width = screen_width - 1;
        let screen_height = screen_height - start_y;
        queue!(
            stdout,
            cursor::MoveTo(start_x, start_y),
            terminal::Clear(terminal::ClearType::FromCursorDown)
        )?;

        let mut draws = if let Some(calls) = &tab.cached_draw {
            calls.clone()
        } else {
            let mut global_ctx = GlobalDrawContext {
                unknown_sized_elements: Vec::new(),
                global_style: &tab.global_style,
                interactable_index: 0,
            };
            let mut draw_data = DrawData {
                parent_width: ActualMeasurement::Pixels(screen_width * EM),
                parent_height: ActualMeasurement::Pixels(screen_height * LH),
                ..Default::default()
            };
            tab.root
                .as_ref()
                .unwrap()
                .draw(DEFAULT_DRAW_CTX, &mut global_ctx, &mut draw_data)?;

            // sort draw calls such that rect calls are drawn first
            draw_data.draw_calls.sort_by_key(|a| a.order());
            // reverse because vecs are LIFO
            draw_data.draw_calls.reverse();
            let draws = CachedDraw {
                calls: draw_data.draw_calls,
                unknown_sized_elements: global_ctx.unknown_sized_elements,
                content_height: draw_data.content_height,
            };
            tab.cached_draw = Some(draws.clone());
            draws
        };
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
                ActualMeasurement::Waiting(i) => {
                    if let ActualMeasurement::Pixels(p) = unknown_sized_elements[i].unwrap() {
                        p
                    } else {
                        panic!("Unresolved ActualMeasurement::Waiting")
                    }
                }
            }
        }
        tab.hovered_interactable = None;

        while let Some(call) = draws.calls.pop() {
            match call {
                DrawCall::Rect(x, y, w, h, color) => {
                    let x = x / EM + start_x;
                    let mut y = y / LH + start_y;

                    let w = actualize_actual(w, &draws.unknown_sized_elements);
                    let h = actualize_actual(h, &draws.unknown_sized_elements);
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
                DrawCall::Image(x, y, w, h, source) => {
                    let Ok(url) = Url::options().base_url(tab.url.as_ref()).parse(&source) else {
                        continue;
                    };
                    let Some(DataEntry::Image(image)) = self.fetched_assets.get(&url) else {
                        continue;
                    };
                    let w = actualize_actual(w, &draws.unknown_sized_elements);
                    let h = actualize_actual(h, &draws.unknown_sized_elements);
                    let x = x / EM + start_x;
                    let mut y = y / LH + start_y;
                    let w = w / EM;
                    let mut h = h / LH;
                    let image = image.resize_exact(
                        w as u32,
                        h as u32 * 2,
                        image::imageops::FilterType::Nearest,
                    );

                    let bottom_out = y - start_y < tab.scroll_y;
                    let mut image_row_offset = 0;

                    if bottom_out && y + h - start_y < tab.scroll_y {
                        continue;
                    } else if bottom_out {
                        let o = y;
                        y = start_y + tab.scroll_y;
                        h -= y - o;
                        image_row_offset += (y - o) * 2;
                    } else if y - tab.scroll_y > (screen_height + start_y) {
                        continue;
                    } else if y + h - tab.scroll_y > (screen_height + start_y) {
                        h = (screen_height + start_y) + tab.scroll_y - y;
                    }

                    let y = y.saturating_sub(tab.scroll_y);
                    for i in (0..h as u32 * 2).step_by(2) {
                        queue!(stdout, cursor::MoveTo(x, y + i as u16 / 2))?;
                        hamis::draw_row(
                            &mut stdout,
                            &image,
                            i + image_row_offset as u32,
                            1,
                            Some(DEFAULT_BACKGROUND_COLOR),
                        )?;

                        actual_cursor_x = x + w;
                        actual_cursor_y = y + h;
                    }
                }
                DrawCall::Text(x, y, text, mut ctx, parent_width, parent_interactable) => {
                    if let Some(interactable) = parent_interactable
                        && let Some(tab_amt) = tab.tab_index
                        && tab_amt == interactable.index
                    {
                        tab.hovered_interactable = Some(interactable);
                        ctx.background_color = Specified(style::Color::Blue);
                    }
                    apply_draw_ctx(ctx, &mut last, &mut stdout.lock())?;
                    let width = actualize_actual(parent_width, &draws.unknown_sized_elements) / EM;
                    for (index, line) in text.lines().enumerate() {
                        let text_len = line.len() as u16;
                        let x = x / EM + start_x;

                        let offset_x = match ctx.text_align {
                            Some(TextAlignment::Centre) if width > x + text_len => {
                                (width - x) / 2 - text_len / 2
                            }
                            Some(TextAlignment::Right) if width > text_len => width - text_len,
                            _ => 0,
                        };
                        let x = x + offset_x;
                        let y = y / LH + index as u16 + start_y;
                        if y - start_y < tab.scroll_y
                            || y - tab.scroll_y >= (screen_height + start_y)
                        {
                            continue;
                        }
                        if x + text_len > screen_width {
                            if x >= screen_width {
                                continue;
                            }
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

        if draws.content_height / LH > screen_height {
            // draw scrollbar
            let scroll_amt = ((tab.scroll_y * LH) as f32
                / (draws.content_height - screen_height) as f32)
                .min(1.0)
                * screen_height as f32;
            queue!(
                stdout,
                cursor::MoveTo(screen_width - 1, start_y + scroll_amt as u16),
                style::SetForegroundColor(style::Color::Black),
            )?;
            print!("█");
        }
        queue!(stdout, style::ResetColor)
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut toad = Toad::new().unwrap();
    toad.open_page(parse_html(include_str!("home.html")).unwrap());
    toad.open_page(parse_html(include_str!("test.html")).unwrap());
    toad.tab_index = 0;
    toad.run().await
}

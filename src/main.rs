use base64::{Engine, prelude::BASE64_STANDARD};
use crossterm::{cursor, event, execute, queue, style, terminal};
use reqwest::{Client, Method, Url};
use std::{
    borrow::Cow,
    collections::HashMap,
    fmt::Debug,
    io::{self, Stdout, Write, stdout},
    str::FromStr,
    time::Duration,
};
use tokio::task::JoinHandle;

use buffer::*;
use consts::*;
use element::*;
use parsing::*;
use utils::*;

mod buffer;
mod consts;
mod css;
mod element;
mod parsing;
mod utils;

#[derive(Clone)]
struct CachedDraw {
    calls: Vec<DrawCall>,
    unknown_sized_elements: Vec<Option<ActualMeasurement>>,
    interactables: Vec<Interactable>,
    content_height: u16,
    forms: Vec<Form>,
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
    hovered_interactable: Option<Interactable>,
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
    /// X, Y, Text, DrawContext, Parent Width, Parent Interactable
    Text(
        u16,
        u16,
        String,
        ElementDrawContext,
        ActualMeasurement,
        Option<usize>,
    ),
    /// X, Y, W, H, Interactable Index, Placeholder Text
    DrawInput(
        u16,
        u16,
        ActualMeasurement,
        ActualMeasurement,
        usize,
        String,
    ),
    ClearColor(style::Color),
}
impl DrawCall {
    fn order(&self) -> u8 {
        match self {
            DrawCall::ClearColor(_) => 0,
            DrawCall::Rect(_, _, _, _, _) => 1,
            DrawCall::Image(_, _, _, _, _) => 2,
            DrawCall::DrawInput(_, _, _, _, _, _) => 3,
            DrawCall::Text(_, _, _, _, _, _) => 4,
        }
    }
}
impl Debug for DrawCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DrawCall::ClearColor(color) => f.write_str(&format!("Clear({color:?})")),
            DrawCall::DrawInput(x, y, w, h, _, _) => {
                f.write_str(&format!("Input({x},{y},{w:?},{h:?})"))
            }
            DrawCall::Image(x, y, w, h, source) => {
                f.write_str(&format!("Image({x},{y},{w:?},{h:?},{source:?})"))
            }
            DrawCall::Rect(x, y, w, h, c) => {
                f.write_str(&format!("Rect({x},{y},{w:?},{h:?},{c:?})"))
            }
            DrawCall::Text(x, y, text, _, _, _) => f.write_str(&format!("Text({x},{y},'{text}')")),
        }
    }
}

#[derive(Clone, Default)]
struct Form {
    action: String,
    method: Method,
    text_fields: HashMap<String, String>,
}

#[derive(Clone, PartialEq)]
enum Interactable {
    Link(String),
    InputText(usize, String),
    InputSubmit(usize),
}
struct GlobalDrawContext<'a> {
    /// The global CSS stylesheet
    global_style: &'a Vec<(StyleTarget, ElementDrawContext)>,
    /// Buffer that all elements with unknown sizes are added to, such that any relative size to an unknown can later be evaluated.
    unknown_sized_elements: Vec<Option<ActualMeasurement>>,
    /// Keeps track of interactable elements
    interactables: Vec<Interactable>,

    forms: Vec<Form>,
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
    redirect_to: Option<String>,
}
impl Debug for WebpageDebugInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut log = String::new();
        for item in self.info_log.iter() {
            log += &format!("-{:?}\n", item);
        }
        write!(
            f,
            "Info Log: \n\n{log}\n\nUnknown elements: {:?}\n\nRedirect to: {:?}",
            self.unknown_elements, self.redirect_to
        )
    }
}

fn actualize_actual(
    a: ActualMeasurement,
    unknown_sized_elements: &Vec<Option<ActualMeasurement>>,
) -> u16 {
    match a {
        ActualMeasurement::Pixels(p) => p,
        ActualMeasurement::PercentOfUnknown(i, p) => {
            (actualize_actual(unknown_sized_elements[i].unwrap(), unknown_sized_elements) as f32
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

const DEBUG_PAGE: &str = include_str!("debug.html");

fn parse_base64_url(url: &Url) -> Option<Vec<u8>> {
    if url.scheme() == "data"
        && let Some((_, base64)) = remove_whitespace(url.path()).split_once(',')
        && let Ok(data) = BASE64_STANDARD.decode(base64)
    {
        Some(data)
    } else {
        None
    }
}

async fn get_data(url: Url, ty: DataType, client: Client) -> Option<DataEntry> {
    if let DataType::Image = ty
        && let Some(data) = parse_base64_url(&url)
    {
        let image = image::load_from_memory(&data).ok()?;
        return Some(DataEntry::Image(image));
    }

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
    cached_resized_images: Vec<(Url, u16, u16, image::DynamicImage)>,
    prev_buffer: Option<Buffer>,
}
impl Toad {
    fn new() -> Result<Self, reqwest::Error> {
        // i stole the firefox user agent,
        // because i was scared websites would think my program was a scraper bot if i had something too unique
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
    async fn open_page(&mut self, mut page: Webpage) {
        if !self.tabs.is_empty() {
            self.tab_index += 1;
        }
        let url = page.url.as_ref().cloned();
        let options = Url::options().base_url(url.as_ref());
        if let Some(redirect) = &page.debug_info.redirect_to
            && let Ok(url) = options.parse(redirect)
            && let Some(new) = self.get_url(url).await
        {
            page = new;
        }

        refresh_style(&mut page, &self.fetched_assets);
        page.indentifier = self.current_page_id;
        for (ty, source) in page.debug_info.fetch_queue.drain(..) {
            let Ok(url) = options.parse(&source) else {
                continue;
            };
            let handle = tokio::spawn(get_data(url.clone(), ty, self.client.clone()));
            self.fetches.push((page.indentifier, url, handle));
        }
        self.current_page_id += 1;
        self.tabs.insert(self.tab_index, page);
    }
    async fn interact(&mut self, mut stdout: &Stdout) -> io::Result<()> {
        let Some(tab) = self.tabs.get_mut(self.tab_index) else {
            return Ok(());
        };
        let Some(hovered) = &tab.hovered_interactable else {
            return Ok(());
        };
        match &hovered {
            Interactable::Link(path) => {
                let options = Url::options().base_url(tab.url.as_ref());
                let Ok(url) = options.parse(path) else {
                    return Ok(());
                };
                if let Some(page) = self.get_url(url).await {
                    self.open_page(page).await;
                }

                self.draw_topbar(stdout)?;
                self.draw(stdout)?;
            }
            Interactable::InputText(index, name) => {
                let Some(cached) = &mut tab.cached_draw else {
                    return Ok(());
                };
                let input = get_line_input(&mut stdout, 0, 2)?;
                cached.forms[*index].text_fields.insert(name.clone(), input);
                self.prev_buffer = None;
                self.draw(stdout)?;
            }
            Interactable::InputSubmit(index) => {
                let Some(mut cached) = tab.cached_draw.take() else {
                    return Ok(());
                };
                let options = Url::options().base_url(tab.url.as_ref());
                let a = cached.forms.remove(*index);
                let Ok(url) = options.parse(&a.action) else {
                    return Ok(());
                };

                let Ok(response) = self
                    .client
                    .request(a.method, url.clone())
                    .form(&a.text_fields)
                    .send()
                    .await
                else {
                    return Ok(());
                };
                let Ok(data) = response.text().await else {
                    return Ok(());
                };
                let Some(mut page) = parse_html(&data) else {
                    return Ok(());
                };
                page.url = Some(url);
                self.tabs.remove(self.tab_index);
                self.tab_index = self.tab_index.saturating_sub(1);
                self.open_page(page).await;
            }
        }

        Ok(())
    }
    async fn run(&mut self) -> io::Result<()> {
        add_panic_handler();
        let mut running = true;
        let mut stdout = stdout();
        terminal::enable_raw_mode()?;
        queue!(stdout, cursor::Hide, event::EnableMouseCapture)?;
        self.draw_topbar(&stdout)?;
        self.draw(&stdout)?;
        while running {
            if event::poll(Duration::from_millis(100))? {
                let event = event::read()?;
                if !event.is_key_press() {
                    if let event::Event::Mouse(mouse_event) = event {
                        let Some(tab) = self.tabs.get_mut(self.tab_index) else {
                            continue;
                        };
                        let Some(cached) = &tab.cached_draw else {
                            continue;
                        };
                        let Some(prev) = &self.prev_buffer else {
                            continue;
                        };
                        if mouse_event.row < 2 {
                            continue;
                        }
                        let mut needs_redraw = false;

                        let cursor_item = prev.get_interactable(
                            mouse_event.column as usize,
                            mouse_event.row as usize - 2,
                        );

                        let new = cursor_item.map(|f| cached.interactables[f].clone());
                        if tab.tab_index != cursor_item {
                            tab.tab_index = cursor_item;
                            tab.hovered_interactable = new;
                            needs_redraw = true;
                        }
                        match mouse_event.kind {
                            event::MouseEventKind::ScrollDown => {
                                tab.scroll_y += 1;
                                needs_redraw = true;
                            }
                            event::MouseEventKind::ScrollUp => {
                                tab.scroll_y = tab.scroll_y.saturating_sub(1);
                                needs_redraw = true;
                            }
                            event::MouseEventKind::Down(_) => {
                                self.interact(&stdout).await?;
                                needs_redraw = false;
                            }
                            _ => {}
                        }
                        if needs_redraw {
                            self.draw(&stdout)?;
                        }
                    };
                    continue;
                }
                let event::Event::Key(key) = event else {
                    continue;
                };
                match key.code {
                    event::KeyCode::Enter => {
                        self.interact(&stdout).await?;
                    }
                    event::KeyCode::F(12) => {
                        if let Some(tab) = self.tabs.get(self.tab_index) {
                            let debug = tab.debug_info.clone();
                            let _page_text = sanitize(
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
                               // .replace("{PAGE}", &page_text);
                               ;
                            if let Some(page) = parse_html(&html) {
                                self.open_page(page).await;
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
                                self.prev_buffer = None;
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
                            let input = get_line_input(&mut stdout, 0, 1)?;
                            if let Ok(url) = Url::from_str(&input)
                                && let Some(page) = self.get_url(url).await
                            {
                                self.open_page(page).await;
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
        let screen_height = screen_height - start_y;
        queue!(stdout, cursor::MoveTo(start_x, start_y))?;

        let mut draws = if let Some(calls) = &tab.cached_draw {
            calls.clone()
        } else {
            let mut global_ctx = GlobalDrawContext {
                unknown_sized_elements: Vec::new(),
                global_style: &tab.global_style,
                interactables: Vec::new(),
                forms: Vec::new(),
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
                interactables: global_ctx.interactables,
                forms: global_ctx.forms,
            };
            tab.cached_draw = Some(draws.clone());
            draws
        };

        tab.hovered_interactable = None;
        let mut buffer = Buffer::empty(screen_width, screen_height);

        while let Some(call) = draws.calls.pop() {
            match call {
                DrawCall::ClearColor(color) => {
                    buffer.clear_color(color);
                }
                DrawCall::Rect(x, y, w, h, color) => {
                    let x = x / EM;
                    let mut y = y / LH;

                    let w = actualize_actual(w, &draws.unknown_sized_elements);
                    let h = actualize_actual(h, &draws.unknown_sized_elements);
                    let w = w / EM;
                    let mut h = h / LH;
                    let bottom_out = y < tab.scroll_y;

                    if bottom_out && y + h < tab.scroll_y {
                        continue;
                    } else if bottom_out {
                        let o = y;
                        y = tab.scroll_y;
                        h -= y - o;
                    } else if y - tab.scroll_y > (screen_height) {
                        continue;
                    } else if y + h - tab.scroll_y > (screen_height) {
                        h = screen_height + tab.scroll_y - y;
                    }
                    y -= tab.scroll_y;

                    buffer.draw_rect(x, y, w, h, color);
                }
                DrawCall::Image(x, y, w, h, source) => {
                    let Ok(url) = Url::options().base_url(tab.url.as_ref()).parse(&source) else {
                        continue;
                    };
                    let Some(DataEntry::Image(image)) = self.fetched_assets.get(&url) else {
                        continue;
                    };
                    let x = x / EM;
                    let mut y = y / LH;

                    let w = actualize_actual(w, &draws.unknown_sized_elements);
                    let h = actualize_actual(h, &draws.unknown_sized_elements);
                    let w = w / EM;
                    let mut h = h / LH;

                    // we need to resize the source image.
                    // either it has already been resized and cached previously,
                    // or we have to resize it now and cache it.
                    let image: Cow<'_, image::DynamicImage> = if let Some((_, _, _, image)) = self
                        .cached_resized_images
                        .iter()
                        .find(|(u, cw, ch, _)| *u == url && *cw == w && *ch == h)
                    {
                        Cow::Borrowed(image)
                    } else {
                        let image = image.resize_exact(
                            w as u32,
                            h as u32 * 2,
                            image::imageops::FilterType::Nearest,
                        );
                        self.cached_resized_images
                            .push((url.clone(), w, h, image.clone()));
                        Cow::Owned(image)
                    };

                    let bottom_out = y < tab.scroll_y;
                    let mut image_row_offset = 0;

                    if bottom_out && y + h < tab.scroll_y {
                        continue;
                    } else if bottom_out {
                        let o = y;
                        y = tab.scroll_y;
                        h -= y - o;
                        image_row_offset += (y - o) * 2;
                    } else if y - tab.scroll_y > screen_height {
                        continue;
                    } else if y + h - tab.scroll_y > (screen_height) {
                        h = (screen_height) + tab.scroll_y - y;
                    }

                    let y = y.saturating_sub(tab.scroll_y);
                    for i in (0..h as u32 * 2).step_by(2) {
                        buffer.draw_img_row(
                            x,
                            y + i as u16 / 2,
                            i + image_row_offset as u32,
                            &image,
                        );
                    }
                }
                DrawCall::DrawInput(x, y, w, h, interactable_index, mut placeholder_text) => {
                    let x = x / EM;
                    let mut y = y / LH;

                    let w = actualize_actual(w, &draws.unknown_sized_elements);
                    let h = actualize_actual(h, &draws.unknown_sized_elements);
                    let w = w / EM;
                    let mut h = h / LH;

                    let bottom_out = y < tab.scroll_y;
                    let mut image_row_offset = 0;

                    if bottom_out && y + h < tab.scroll_y {
                        continue;
                    } else if bottom_out {
                        let o = y;
                        y = tab.scroll_y;
                        h -= y - o;
                        image_row_offset += (y - o) * 2;
                    } else if y - tab.scroll_y > screen_height {
                        continue;
                    } else if y + h - tab.scroll_y > (screen_height) {
                        h = (screen_height) + tab.scroll_y - y;
                    }

                    let hovered = tab.tab_index.is_some_and(|f| f == interactable_index);
                    let interactable = &draws.interactables[interactable_index];
                    let (form, name) = match interactable {
                        Interactable::InputText(form, text) => (form, text.clone()),
                        Interactable::InputSubmit(form) => (form, String::from("Submit Button")),
                        _ => {
                            panic!()
                        }
                    };
                    let form = &draws.forms[*form];
                    if hovered {
                        tab.hovered_interactable = Some(interactable.clone());
                    }
                    if let Some(value) = form.text_fields.get(&name) {
                        placeholder_text = value.clone();
                    }

                    let y = y.saturating_sub(tab.scroll_y);
                    for i in 0..h {
                        buffer.draw_input_box(
                            x,
                            y + i,
                            i + image_row_offset,
                            w,
                            h + image_row_offset,
                            &placeholder_text,
                            hovered,
                            interactable_index,
                        );
                    }
                }
                DrawCall::Text(x, y, text, mut ctx, parent_width, parent_interactable) => {
                    if let Some(interactable) = parent_interactable
                        && let Some(tab_amt) = tab.tab_index
                        && tab_amt == interactable
                    {
                        tab.hovered_interactable = Some(draws.interactables[interactable].clone());
                        ctx.background_color = Specified(style::Color::Blue);
                    }
                    let x = x / EM;
                    let y = y / LH;
                    let width = actualize_actual(parent_width, &draws.unknown_sized_elements) / EM;

                    let text_len = text.len() as u16;

                    let offset_x = match ctx.text_align {
                        Some(TextAlignment::Centre) if width > x + text_len => {
                            (width - x) / 2 - text_len / 2
                        }
                        Some(TextAlignment::Right) if width > text_len => width - text_len,
                        _ => 0,
                    };
                    let x = x + offset_x;

                    if let Some(y) = y.checked_sub(tab.scroll_y) {
                        buffer.draw_str(x, y, &text, &ctx, parent_interactable);
                    }
                }
            }
        }
        if draws.content_height / LH > screen_height {
            // draw scrollbar
            let scroll_amt = (((tab.scroll_y * LH) as f32
                / (draws.content_height - screen_height) as f32)
                .min(1.0)
                * screen_height as f32)
                .min(screen_height as f32 - 1.0);
            buffer.set_pixel(screen_width - 1, scroll_amt as u16, style::Color::Black);
        }

        buffer.render(
            &mut stdout,
            self.prev_buffer.as_ref(),
            start_x as _,
            start_y as _,
        )?;
        self.prev_buffer = Some(buffer);

        queue!(stdout, style::ResetColor)
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut toad = Toad::new().unwrap();
    toad.open_page(parse_html(include_str!("home.html")).unwrap())
        .await;
    toad.open_page(parse_html(include_str!("test.html")).unwrap())
        .await;
    toad.tab_index = 0;
    toad.run().await
}

#[cfg(test)]
mod tests {
    use reqwest::{Client, Url};

    use crate::{DataEntry, DataType, get_data};

    #[tokio::test]
    async fn test_base64_urls() {
        let b64 = "data:image/png;base64, iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAAXNSR0IArs4c6QAAAARnQU1BAACxjwv8
        YQUAAAAJcEhZcwAADsMAAA7DAcdvqGQAAAAYdEVYdFNvZnR3YXJlAFBhaW50Lk5FVCA1LjEuOWxu2j4AAAC2ZVhJZklJKgAIAAAABQAaAQUAAQAAAEoAAAAbAQUAAQAA
        AFIAAAAoAQMAAQAAAAIAAAAxAQIAEAAAAFoAAABphwQAAQAAAGoAAAAAAAAAYAAAAAEAAABgAAAAAQAAAFBhaW50Lk5FVCA1LjEuOQADAACQBwAEAAAAMDIzMAGgAwAB
        AAAAAQAAAAWgBAABAAAAlAAAAAAAAAACAAEAAgAEAAAAUjk4AAIABwAEAAAAMDEwMAAAAABMz8BIJY/XoAAAABdJREFUGFdjZPh/4f+lywz/a14y/L8AADvICKjr7H/4
        AAAAAElFTkSuQmCC";
        let url = Url::parse(b64).unwrap();
        let DataEntry::Image(resp) = get_data(url, DataType::Image, Client::new()).await.unwrap()
        else {
            panic!()
        };

        assert_eq!(
            resp.as_bytes(),
            [0, 255, 208, 255, 209, 163, 255, 124, 233, 0, 255, 208]
        );
    }
}

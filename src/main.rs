use base64::{Engine, prelude::BASE64_STANDARD};
use crossterm::{
    cursor,
    event::{self},
    execute, queue, style, terminal,
};
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
use unicode_width::UnicodeWidthStr;

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
impl Webpage {
    fn get_title(&self) -> String {
        if let Some(title) = self.title.clone() {
            return title;
        }
        if let Some(url) = self.url.clone() {
            return url.to_string();
        }
        String::from("unknown")
    }
}
struct Tab {
    history: Vec<Webpage>,
    future: Vec<Webpage>,
}
impl Tab {
    fn backwards(&mut self) {
        if self.history.len() > 1
            && let Some(p) = self.history.pop()
        {
            self.future.push(p);
        }
    }
    fn forwards(&mut self) {
        if let Some(p) = self.future.pop() {
            self.history.push(p);
        }
    }
    fn page(&self) -> &Webpage {
        self.history.last().unwrap()
    }
    fn page_mut(&mut self) -> &mut Webpage {
        self.history.last_mut().unwrap()
    }
}
#[derive(Default)]
struct TabManager {
    tabs: Vec<Tab>,
}
impl TabManager {
    fn find_identifier_mut(&mut self, identifier: usize) -> Option<&mut Webpage> {
        self.tabs
            .iter_mut()
            .find(|f| {
                let page = f.page();
                page.indentifier == identifier
            })
            .map(|f| f.page_mut())
    }
    fn len(&self) -> usize {
        self.tabs.len()
    }
    fn iter(&self) -> std::slice::Iter<'_, Tab> {
        self.tabs.iter()
    }
    fn get(&self, index: usize) -> Option<&Webpage> {
        self.tabs.get(index).map(|f| f.page())
    }
    fn get_mut(&mut self, index: usize) -> Option<&mut Webpage> {
        self.tabs.get_mut(index).map(|f| f.page_mut())
    }
    fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }
    fn insert(&mut self, index: usize, page: Webpage) {
        self.tabs.insert(
            index,
            Tab {
                history: vec![page],
                future: Vec::new(),
            },
        );
    }
    fn remove(&mut self, index: usize) -> Tab {
        self.tabs.remove(index)
    }
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
    InputText(usize, String, u16, Option<(u16, u16)>),
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
    Webpage(Box<Webpage>),
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

async fn get_page(client: Client, url: Url) -> Option<DataEntry> {
    let response = client.get(url.clone()).send().await.ok()?;
    let data = response.text().await.ok()?;
    let mut page = parse_html(&data)?;
    page.url = Some(url);
    Some(DataEntry::Webpage(Box::new(page)))
}
async fn get_page_with_form(client: Client, url: Url, form: Form) -> Option<DataEntry> {
    let Ok(response) = client
        .request(form.method, url.clone())
        .form(&form.text_fields)
        .send()
        .await
    else {
        return None;
    };
    let Ok(data) = response.text().await else {
        return None;
    };
    let mut page = parse_html(&data)?;
    page.url = Some(url);
    Some(DataEntry::Webpage(Box::new(page)))
}

type FetchFuture = JoinHandle<Option<DataEntry>>;

#[derive(Default)]
struct Toad {
    tabs: TabManager,
    tab_index: usize,
    client: Client,
    fetched_assets: HashMap<Url, DataEntry>,
    fetches: Vec<(usize, Url, FetchFuture)>,
    current_page_id: usize,
    cached_resized_images: Vec<(Url, u16, u16, image::DynamicImage)>,
    prev_buffer: Option<Buffer>,
    current_input_box: Option<InputBox>,
    last_mouse_x: u16,
    last_mouse_y: u16,
}
impl Toad {
    fn new() -> Result<Self, reqwest::Error> {
        // maybe ill change this to spoof user agent with that of firefox,
        // to prevent websites thinking this is a scraper bot.
        // (if found necessary)
        let client = Client::builder()
            .user_agent(format!("Toad/{}", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            client,
            ..Default::default()
        })
    }
    async fn handle_new_page(&mut self, page: &mut Webpage) {
        let url = page.url.as_ref().cloned();
        let options = Url::options().base_url(url.as_ref());
        if let Some(redirect) = &page.debug_info.redirect_to
            && let Ok(url) = options.parse(redirect)
        {
            let handle = tokio::spawn(get_page(self.client.clone(), url.clone()));
            self.fetches
                .push((self.current_page_id, url.clone(), handle));
        }

        refresh_style(page, &self.fetched_assets);
        page.indentifier = self.current_page_id;
        self.current_page_id += 1;
        for (ty, source) in page.debug_info.fetch_queue.drain(..) {
            let Ok(url) = options.parse(&source) else {
                continue;
            };
            if !self.fetched_assets.contains_key(&url) {
                let handle = tokio::spawn(get_data(url.clone(), ty, self.client.clone()));
                self.fetches.push((page.indentifier, url, handle));
            }
        }
    }
    async fn open_page(&mut self, mut page: Webpage, tab_index: usize) {
        self.handle_new_page(&mut page).await;
        let tab = &mut self.tabs.tabs[tab_index];
        tab.history.push(page);
        tab.future.clear();
    }
    async fn open_page_new_tab(&mut self, mut page: Webpage) {
        if !self.tabs.is_empty() {
            self.tab_index += 1;
        }
        self.handle_new_page(&mut page).await;
        self.tabs.insert(self.tab_index, page);
    }
    async fn interact(&mut self, stdout: &Stdout, control_held: bool) -> io::Result<()> {
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
                let handle = tokio::spawn(get_page(self.client.clone(), url.clone()));
                self.fetches
                    .push((self.current_page_id, url.clone(), handle));
                let mut page = parse_html("<html></html>").unwrap();
                page.url = Some(url);
                if control_held {
                    self.open_page_new_tab(page).await;
                } else {
                    self.open_page(page, self.tab_index).await;
                }

                self.draw(stdout)?;
            }
            Interactable::InputText(index, name, width, pos) => {
                let Some(cached) = &mut tab.cached_draw else {
                    return Ok(());
                };
                let (x, y) = pos.unwrap();
                self.current_input_box = Some(InputBox::new(
                    x + 1,
                    y + 1,
                    *width,
                    InputBoxSubmitTarget::SetFormTextField(*index, name.clone()),
                    cached.forms[*index].text_fields.get(name).cloned(),
                ));
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

                let handle = tokio::spawn(get_page_with_form(self.client.clone(), url.clone(), a));
                self.fetches
                    .push((self.current_page_id, url.clone(), handle));
                let mut page = parse_html("<html></html>").unwrap();
                page.url = Some(url);
                self.open_page(page, self.tab_index).await;
                self.draw(stdout)?;
            }
        }

        Ok(())
    }
    async fn handle_input_box_state(&mut self, mut stdout: &Stdout) -> io::Result<()> {
        let input_box = self.current_input_box.as_mut().unwrap();
        match &input_box.state {
            InputBoxState::Submitted => {
                let input_box = self.current_input_box.take().unwrap();
                queue!(stdout, cursor::Hide)?;
                self.prev_buffer = None;
                match input_box.on_submit {
                    InputBoxSubmitTarget::ChangeAddress | InputBoxSubmitTarget::OpenNewTab => {
                        if let Ok(url) = Url::from_str(&input_box.text) {
                            let handle = tokio::spawn(get_page(self.client.clone(), url.clone()));
                            self.fetches
                                .push((self.current_page_id, url.clone(), handle));
                            let mut page = parse_html("<html></html>").unwrap();
                            page.url = Some(url);
                            self.open_page(page, self.tab_index).await;
                        } else if let InputBoxSubmitTarget::OpenNewTab = input_box.on_submit {
                            self.tabs.remove(self.tab_index);
                            self.tab_index = self.tab_index.saturating_sub(1);
                        }
                        self.draw(stdout)?;
                    }
                    InputBoxSubmitTarget::SetFormTextField(index, name) => {
                        if let Some(tab) = self.tabs.get_mut(self.tab_index)
                            && let Some(cached) = &mut tab.cached_draw
                        {
                            cached.forms[index]
                                .text_fields
                                .insert(name.clone(), input_box.text);
                        };
                        self.draw(stdout)?;
                    }
                }
            }
            InputBoxState::Cancelled => {
                let input_box = self.current_input_box.take().unwrap();
                queue!(stdout, cursor::Hide)?;
                self.prev_buffer = None;
                if let InputBoxSubmitTarget::SetFormTextField(_, _) = input_box.on_submit {
                    self.prev_buffer = None;
                }
                if let InputBoxSubmitTarget::OpenNewTab = input_box.on_submit {
                    self.tabs.remove(self.tab_index);
                    self.tab_index = self.tab_index.saturating_sub(1);
                }
                self.draw(stdout)?;
            }
            _ => {
                self.draw(stdout)?;
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
        self.draw(&stdout)?;
        while running {
            let (screen_width, _) = terminal::size()?;
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
                        (self.last_mouse_x, self.last_mouse_y) =
                            (mouse_event.column, mouse_event.row);

                        if self.current_input_box.is_some() {
                            if let event::MouseEventKind::Down(_) = mouse_event.kind
                                && let Some(input_box) = &mut self.current_input_box
                            {
                                input_box.state = InputBoxState::Cancelled;
                                self.handle_input_box_state(&stdout).await?;
                            }
                        } else {
                            let mut needs_redraw = false;

                            if mouse_event.row >= 3 {
                                let cursor_item = prev.get_interactable(
                                    mouse_event.column as usize,
                                    mouse_event.row as usize,
                                );

                                let new = cursor_item.map(|f| cached.interactables[f].clone());
                                if tab.tab_index != cursor_item {
                                    tab.tab_index = cursor_item;
                                    tab.hovered_interactable = new;
                                    needs_redraw = true;
                                }
                            } else {
                                needs_redraw = true;
                                tab.hovered_interactable = None;
                                tab.tab_index = None;
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
                                    if mouse_event.row >= 3 {
                                        // handle click interactable
                                        self.interact(
                                            &stdout,
                                            mouse_event
                                                .modifiers
                                                .contains(event::KeyModifiers::CONTROL),
                                        )
                                        .await?;
                                        needs_redraw = false;
                                    } else if mouse_event.row == 0 {
                                        let screen_width = terminal::size()?.0 as usize;
                                        let mut current_tab_width = self
                                            .tabs
                                            .get(self.tab_index)
                                            .unwrap()
                                            .get_title()
                                            .trim()
                                            .width()
                                            + 3;
                                        if current_tab_width > screen_width - 3 {
                                            current_tab_width = screen_width - 3;
                                        }
                                        let other_space = screen_width - current_tab_width;
                                        let max_invidivual_tab_width = if self.tabs.len() <= 1 {
                                            0
                                        } else {
                                            other_space / (self.tabs.len() - 1)
                                        };
                                        // click tab bar
                                        let mouse_x = mouse_event.column as usize;
                                        let mut x = 0;
                                        for (index, tab) in self.tabs.iter().enumerate() {
                                            let page = tab.page();
                                            let text = page.get_title().trim().to_string();
                                            let w = text.width();
                                            let width = if index == self.tab_index {
                                                current_tab_width - 3
                                            } else {
                                                if max_invidivual_tab_width <= 3 {
                                                    continue;
                                                }
                                                w.min(max_invidivual_tab_width - 3)
                                            };
                                            let old = x;
                                            x += width + 3;
                                            if (old..x).contains(&mouse_x) {
                                                self.tab_index = index;
                                                needs_redraw = true;
                                                break;
                                            }
                                        }
                                    } else if mouse_event.row == 1 {
                                        if mouse_event.column >= 4 * 3
                                            && mouse_event.column < screen_width - 4 * 3
                                        {
                                            // click url bar

                                            self.current_input_box = Some(InputBox::new(
                                                4 * 3,
                                                1,
                                                screen_width - 4 * 3 * 2,
                                                InputBoxSubmitTarget::ChangeAddress,
                                                tab.url.clone().map(|f| f.to_string()),
                                            ));
                                            needs_redraw = true;
                                        } else if mouse_event.column <= 2 {
                                            self.tabs.tabs[self.tab_index].backwards();
                                        } else if mouse_event.column <= 5 {
                                            self.tabs.tabs[self.tab_index].forwards();
                                        } else if mouse_event.column > 6 && mouse_event.column <= 9
                                        {
                                            if let Some(page) = self.tabs.get_mut(self.tab_index) {
                                                page.scroll_y = 0;
                                                refresh_style(page, &self.fetched_assets);
                                                page.cached_draw = None;
                                                self.prev_buffer = None;
                                            }
                                            self.draw(&stdout)?;
                                        }
                                    }
                                }
                                _ => {}
                            }
                            if needs_redraw {
                                self.draw(&stdout)?;
                            }
                        }
                    };
                    continue;
                }
                let event::Event::Key(key) = event else {
                    continue;
                };
                if let Some(input_box) = &mut self.current_input_box {
                    input_box.on_event(key);
                    self.handle_input_box_state(&stdout).await?;
                } else {
                    match key.code {
                        event::KeyCode::Enter => {
                            self.interact(
                                &stdout,
                                key.modifiers.contains(event::KeyModifiers::CONTROL),
                            )
                            .await?;
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
                                    self.open_page_new_tab(page).await;
                                    self.draw(&stdout)?;
                                }
                            }
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
                        event::KeyCode::Right => {
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) {
                                self.tabs.tabs[self.tab_index].forwards();
                                self.draw(&stdout)?;
                            } else if let Some(tab) = self.tabs.get_mut(self.tab_index) {
                                tab.tab_index = Some(tab.tab_index.map(|i| i + 1).unwrap_or(0));
                                self.draw(&stdout)?;
                            }
                        }
                        event::KeyCode::Left => {
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) {
                                self.tabs.tabs[self.tab_index].backwards();
                                self.draw(&stdout)?;
                            } else if let Some(tab) = self.tabs.get_mut(self.tab_index) {
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
                            if char == 'q' {
                                running = false;
                            } else if char == 'w' && control {
                                if self.tab_index < self.tabs.len() {
                                    self.tabs.remove(self.tab_index);
                                    self.tab_index = self.tab_index.saturating_sub(1);
                                    if self.tabs.is_empty() {
                                        break;
                                    }
                                    self.draw(&stdout)?;
                                }
                            } else if char == 't' && control {
                                self.open_page_new_tab(parse_html("<html></html>").unwrap())
                                    .await;
                                self.current_input_box = Some(InputBox::new(
                                    4 * 3,
                                    1,
                                    screen_width - 4 * 3 * 2,
                                    InputBoxSubmitTarget::OpenNewTab,
                                    None,
                                ));
                                self.draw(&stdout)?;
                            }
                        }
                        _ => {}
                    }
                }
            }
            // update fetch queue

            let mut any_changed = false;
            let mut death_queue = Vec::new();

            let mut unhandled_pages = Vec::new();

            for (index, (page_id, url, handle)) in self.fetches.iter_mut().enumerate() {
                if handle.is_finished() {
                    let Ok(polled) = tokio::join!(handle).0 else {
                        continue;
                    };
                    death_queue.push(index);
                    let Some(data) = polled else {
                        if let Some(page) = self.tabs.find_identifier_mut(*page_id) {
                            page.debug_info
                                .info_log
                                .push(format!("Failed to get data of {url}"));
                        }
                        continue;
                    };
                    any_changed = true;
                    if let DataEntry::Webpage(webpage) = data {
                        unhandled_pages.push((*page_id, webpage));
                    } else {
                        self.fetched_assets.insert(url.clone(), data);

                        // refresh page with this page_id
                        if let Some(page) = self.tabs.find_identifier_mut(*page_id) {
                            refresh_style(page, &self.fetched_assets);
                            page.cached_draw = None;
                        }
                    }
                }
            }
            let mut index = 0;
            self.fetches.retain(|_| {
                let old = index;
                index += 1;
                !death_queue.contains(&old)
            });

            for (id, mut page) in unhandled_pages.into_iter() {
                self.handle_new_page(&mut page).await;
                if let Some(p) = self.tabs.find_identifier_mut(id) {
                    *p = *page;
                }
            }

            // if any finished loading
            if any_changed {
                self.draw(&stdout)?;
            }
        }
        terminal::disable_raw_mode()?;

        // clean up styling and move cursor to bottom of screen
        let h = terminal::size()?.1;
        execute!(
            stdout,
            cursor::Show,
            cursor::MoveTo(0, h - 3),
            event::DisableMouseCapture
        )?;
        Ok(())
    }
    fn draw(&mut self, mut stdout: &Stdout) -> io::Result<()> {
        self.draw_current_page(stdout)?;
        if let Some(input_box) = &self.current_input_box {
            input_box.draw(stdout)?;
        }
        stdout.flush()
    }
    fn draw_topbar(&self, buffer: &mut Buffer) {
        let screen_width = terminal::size().unwrap().0 as usize;
        let mut current_tab_width = self
            .tabs
            .get(self.tab_index)
            .unwrap()
            .get_title()
            .trim()
            .width()
            + 3;
        if current_tab_width > screen_width - 3 {
            current_tab_width = screen_width - 3;
        }
        let other_space = screen_width - current_tab_width;
        let max_invidivual_tab_width = if self.tabs.len() <= 1 {
            0
        } else {
            other_space / (self.tabs.len() - 1)
        };
        buffer.draw_rect(0, 0, screen_width as _, 3, GREY_COLOR);
        let mut x = 0;
        for (index, tab) in self.tabs.iter().enumerate() {
            let page = tab.page();
            let mut text = page.get_title().trim().to_string();
            let w = text.width();
            if index == self.tab_index {
                if w > current_tab_width - 3 {
                    text = text[..current_tab_width - 3].to_string();
                }
            } else {
                if max_invidivual_tab_width <= 3 {
                    continue;
                }
                if w > max_invidivual_tab_width - 3 {
                    text = text[..max_invidivual_tab_width - 3].to_string();
                }
            }
            let w = w as u16;
            if index == self.tab_index {
                buffer.draw_rect(x, 0, w + 2, 1, WHITE_COLOR);
            }
            buffer.draw_str(x, 0, &format!("[{text}]"), &DEFAULT_DRAW_CTX, None);
            x += w + 3;
        }
        buffer.draw_rect(4 * 3, 1, screen_width as u16 - 4 * 3 * 2, 1, WHITE_COLOR);
        if let Some(Some(url)) = self.tabs.get(self.tab_index).map(|f| &f.url) {
            let mut text = url.to_string().trim().to_string();
            let w = text.width();
            if w > screen_width {
                text = text[..screen_width].to_string();
            }
            buffer.draw_str(4 * 3, 1, &text, &DEFAULT_DRAW_CTX, None);
        }

        if self.last_mouse_y == 1 {
            if self.last_mouse_x <= 2 {
                buffer.draw_rect(0, 1, 3, 1, WHITE_COLOR);
            } else if self.last_mouse_x <= 5 {
                buffer.draw_rect(3, 1, 3, 1, WHITE_COLOR);
            } else if self.last_mouse_x > 6 && self.last_mouse_x <= 9 {
                buffer.draw_rect(7, 1, 3, 1, WHITE_COLOR);
            }
        }
        buffer.draw_str(0, 1, "[←][→] [↻] ", &DEFAULT_DRAW_CTX, None);
    }
    fn draw_current_page(&mut self, mut stdout: &Stdout) -> io::Result<()> {
        let Some(tab) = self.tabs.get_mut(self.tab_index) else {
            return Ok(());
        };
        let (screen_width, screen_height) = terminal::size()?;

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
                y: 3 * LH,
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
                    let y = y.saturating_sub(tab.scroll_y);

                    let hovered = tab.tab_index.is_some_and(|f| f == interactable_index);
                    let interactable = &draws.interactables[interactable_index];
                    let (form, name) = match interactable {
                        Interactable::InputText(form, text, width, _) => {
                            let new =
                                Interactable::InputText(*form, text.clone(), *width, Some((x, y)));
                            tab.cached_draw.as_mut().unwrap().interactables[interactable_index] =
                                new;

                            (form, text.clone())
                        }
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
                        ctx.background_color = Specified(BLUE_COLOR);
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
            let page_height = screen_height - 3;
            let scroll_amt = (((tab.scroll_y * LH) as f32
                / (draws.content_height - page_height) as f32)
                .min(1.0)
                * page_height as f32)
                .min(page_height as f32 - 1.0);
            buffer.set_pixel(screen_width - 1, scroll_amt as u16 + 3, BLACK_COLOR);
        }

        self.draw_topbar(&mut buffer);

        queue!(stdout, cursor::MoveTo(0, 0))?;
        buffer.render(&mut stdout, self.prev_buffer.as_ref(), 0, 0)?;
        self.prev_buffer = Some(buffer);

        queue!(stdout, style::ResetColor)
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut toad = Toad::new().unwrap();
    toad.fetched_assets.insert(
        Url::parse("toad://toad.png").unwrap(),
        DataEntry::Image(image::load_from_memory(include_bytes!("toad.png")).unwrap()),
    );
    toad.open_page_new_tab(parse_html(include_str!("home.html")).unwrap())
        .await;
    //toad.open_page_new_tab(parse_html(include_str!("test.html")).unwrap())
    //    .await;
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

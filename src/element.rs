use std::{
    collections::HashMap,
    fmt::Debug,
    io::{self, Stdout, Write},
};

use crate::{
    consts::*, css, Display, ElementDrawContext, GlobalDrawContext, Measurement,
    NonInheritedField::*, DEFAULT_DRAW_CTX,
};
use crossterm::{cursor, queue, style, terminal};

const RED: style::Color = style::Color::Red;

#[derive(Clone, Copy, PartialEq)]
pub struct ElementType {
    pub name: &'static str,
    pub stops_parsing: bool,
    /// Element that has no closing tag, such as <img>
    pub void_element: bool,
    pub draw_ctx: ElementDrawContext,
}
pub static DEFAULT_ELEMENT_TYPE: ElementType = ElementType {
    name: "unknown",
    stops_parsing: false,
    void_element: false,
    draw_ctx: DEFAULT_DRAW_CTX,
};
static H1: ElementType = ElementType {
    name: "h1",
    draw_ctx: ElementDrawContext {
        bold: true,
        foreground_color: Some(RED),
        display: Specified(Display::Block),
        ..DEFAULT_DRAW_CTX
    },
    ..DEFAULT_ELEMENT_TYPE
};
pub static ELEMENT_TYPES: &[ElementType] = &[
    ElementType {
        name: "node",
        draw_ctx: ElementDrawContext {
            background_color: Inherit,
            ..DEFAULT_DRAW_CTX
        },
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "html",
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "img",
        void_element: true,
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "br",
        void_element: true,
        draw_ctx: ElementDrawContext {
            display: Specified(Display::Block),
            ..DEFAULT_DRAW_CTX
        },
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "input",
        void_element: true,
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "hr",
        void_element: true,
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "head",
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "body",
        draw_ctx: ElementDrawContext {
            width: Specified(Measurement::PercentWidth(1.0)),
            height: Specified(Measurement::PercentHeight(1.0)),
            background_color: Specified(DEFAULT_BACKGROUND_COLOR),
            display: Specified(Display::Block),
            ..DEFAULT_DRAW_CTX
        },
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "link",
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "title",
        stops_parsing: true,
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "pre",
        draw_ctx: ElementDrawContext {
            respect_whitespace: true,
            display: Specified(Display::Block),
            ..DEFAULT_DRAW_CTX
        },
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "p",
        draw_ctx: ElementDrawContext {
            display: Specified(Display::Block),
            ..DEFAULT_DRAW_CTX
        },
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "span",
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "a",
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "div",
        draw_ctx: ElementDrawContext {
            display: Specified(Display::Block),
            height: Specified(Measurement::FitContentHeight),
            ..DEFAULT_DRAW_CTX
        },
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "style",
        stops_parsing: true,
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "script",
        stops_parsing: true,
        ..DEFAULT_ELEMENT_TYPE
    },
    H1,
    ElementType { name: "h2", ..H1 },
    ElementType { name: "h3", ..H1 },
    ElementType { name: "h4", ..H1 },
    ElementType { name: "h5", ..H1 },
    ElementType { name: "h6", ..H1 },
];
pub fn get_element_type(name: &str) -> Option<&'static ElementType> {
    ELEMENT_TYPES.iter().find(|f| f.name == name)
}
/// Removes repeated whitespace and newlines
fn disrespect_whitespace(text: &str) -> String {
    let text = text.replace("\n", "").replace("\r", "");
    let mut new = String::new();
    let mut last = None;
    for char in text.chars() {
        if char.is_whitespace() {
            if let Some(last_char) = last {
                if last_char == char {
                    continue;
                }
            }
            last = Some(char);
        } else {
            last = None;
        }
        new.push(char);
    }
    new
}
fn is_whitespace(text: &str) -> bool {
    text.chars().all(|c| c.is_whitespace())
}
pub fn apply_draw_ctx<T: Write>(
    draw_ctx: ElementDrawContext,
    old_ctx: &mut ElementDrawContext,
    stdout: &mut T,
) -> io::Result<()> {
    if *old_ctx == draw_ctx {
        return Ok(());
    }
    let needs_clearing = (!draw_ctx.bold && old_ctx.bold) || (!draw_ctx.italics && old_ctx.italics);
    let foreground_color = draw_ctx.foreground_color.unwrap_or(style::Color::Black);
    let background_color = draw_ctx
        .background_color
        .unwrap_or(DEFAULT_BACKGROUND_COLOR);

    if needs_clearing {
        queue!(stdout, style::ResetColor)?;
        old_ctx.bold = false;
        old_ctx.italics = false;
        old_ctx.foreground_color = None;
    }
    let mut attributes = style::Attributes::none();

    if draw_ctx.bold {
        attributes.set(style::Attribute::Bold);
    }
    if draw_ctx.italics {
        attributes.set(style::Attribute::Italic);
    }

    queue!(
        stdout,
        style::SetStyle(style::ContentStyle {
            foreground_color: Some(foreground_color),
            background_color: Some(background_color),
            attributes,
            ..Default::default()
        })
    )?;

    old_ctx.bold = draw_ctx.bold;
    old_ctx.italics = draw_ctx.italics;
    old_ctx.foreground_color = draw_ctx.foreground_color;
    old_ctx.background_color = draw_ctx.background_color;
    Ok(())
}
pub struct Element {
    pub ty: &'static ElementType,
    pub children: Vec<Element>,
    attributes: HashMap<String, String>,
    pub style: ElementDrawContext,
    pub text: Option<String>,
    pub classes: Vec<String>,
}
impl Debug for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.print_recursive(0))
    }
}
impl Element {
    pub fn new(ty: &'static ElementType) -> Self {
        Self {
            ty,
            children: Vec::new(),
            attributes: HashMap::new(),
            classes: Vec::new(),
            style: DEFAULT_DRAW_CTX,
            text: None,
        }
    }
    /// Gets the size of elements content/children by running a dry draw call and comparing how far the cursor is moved.
    pub fn get_content_size<T: Write>(
        &self,
        parent_draw_context: ElementDrawContext,
        global_ctx: &GlobalDrawContext<T>,
    ) -> (u16, u16) {
        let style = self.get_active_style(global_ctx, parent_draw_context);
        let mut fart: Vec<u8> = Vec::new();
        let mut ctx = GlobalDrawContext {
            width: global_ctx.width,
            height: global_ctx.height,
            stdout: &mut fart,
            x: 0,
            y: 0,
            max_x: 0,
            max_y: 0,
            actual_cursor_x: 0,
            actual_cursor_y: 0,
            last_draw_ctx: DEFAULT_DRAW_CTX,
            global_style: &global_ctx.global_style.clone(),
        };
        for child in self.children.iter() {
            let _ = child.draw(style, &mut ctx);
        }
        (ctx.max_x, ctx.max_y + 1)
    }
    pub fn get_attribute(&self, k: &str) -> Option<&String> {
        self.attributes.get(k)
    }
    pub fn set_attributes(&mut self, attributes: HashMap<String, String>) {
        if let Some(style) = attributes.get("style") {
            css::parse_ruleset(style, &mut self.style);
        }
        if let Some(class) = attributes.get("class") {
            self.classes = class.split(' ').map(|f| f.to_string()).collect();
        }
        self.attributes = attributes;
    }
    fn print_recursive(&self, index: usize) -> String {
        let children_text = match &self.text {
            Some(text) => text.clone(),
            None => {
                let mut children_text = String::new();
                for child in &self.children {
                    children_text += "\n";
                    children_text += &child.print_recursive(index + 1);
                }
                children_text
            }
        };
        let padding = "\t".repeat(index);
        let mut attributes = String::new();
        for (k, v) in &self.attributes {
            attributes += &format!(" {k}=\"{v}\"");
        }
        let mut maybe_newline = format!("\n{padding}");
        if !children_text.contains("\n") {
            maybe_newline = String::new();
        }
        format!(
            "\n{padding}<{}{attributes}>{}{maybe_newline}</{}>",
            self.ty.name, children_text, self.ty.name
        )
    }
    pub fn get_active_style<T: Write>(
        &self,
        global_ctx: &GlobalDrawContext<T>,
        parent_draw_context: ElementDrawContext,
    ) -> ElementDrawContext {
        // construct this elements style by overlaying:
        //  - parent style
        //  - the base element's style
        //  - matching global styles
        //  - inline css

        let mut style = self.ty.draw_ctx;
        // merge_inherit will only fill inherited, unset fields of style
        style.merge_inherit(&parent_draw_context);

        for (k, v) in global_ctx.global_style.iter() {
            if k.matches(self) {
                style.merge_all(v);
            }
        }
        style.merge_all(&self.style);

        // check all NonInheritedFields in case they are set to inherit, if so, inherit from parent_draw_context

        style
            .background_color
            .inherit_from(parent_draw_context.background_color);
        style.width.inherit_from(parent_draw_context.width);
        style.height.inherit_from(parent_draw_context.height);
        style.display.inherit_from(parent_draw_context.display);
        style
    }
    pub fn draw<T: Write>(
        &self,
        mut parent_draw_ctx: ElementDrawContext,
        global_ctx: &mut GlobalDrawContext<T>,
    ) -> io::Result<()> {
        // construct this element's active style
        let style = self.get_active_style(global_ctx, parent_draw_ctx);

        if self.ty.stops_parsing || matches!(style.display, Specified(Display::None)) {
            return Ok(());
        }

        let is_body = self.ty.name == "body";

        // hardcoded case for performance
        if is_body {
            let bg_color = style.background_color.unwrap_or(style::Color::White);
            queue!(
                global_ctx.stdout,
                style::SetBackgroundColor(bg_color),
                terminal::Clear(terminal::ClearType::FromCursorDown),
                cursor::MoveTo(global_ctx.x, global_ctx.y)
            )?;
            global_ctx.actual_cursor_x = global_ctx.x;
            global_ctx.actual_cursor_y = global_ctx.y;
        }

        let is_display_block = matches!(style.display, Specified(Display::Block));

        parent_draw_ctx = style;

        if is_display_block && global_ctx.x != 0 {
            global_ctx.y += 1;
            global_ctx.x = 0;
        }
        let screen_size = (global_ctx.width, global_ctx.height);

        let width = style
            .width
            .map(|width| width.to_pixels(screen_size, self, global_ctx, parent_draw_ctx));
        let height = style
            .height
            .map(|height| height.to_pixels(screen_size, self, global_ctx, parent_draw_ctx));
        if !is_body
            && is_display_block
            && width.is_some()
            && height.is_some()
            && style.background_color.is_specified()
        {
            // draw background color over area
            let (old_x, old_y) = (global_ctx.x, global_ctx.y);
            global_ctx.draw_text(
                &(" ".repeat((width.unwrap() / EM) as _) + "\n")
                    .repeat((height.unwrap() / LH) as _),
                style,
            )?;
            (global_ctx.x, global_ctx.y) = (old_x, old_y);
            global_ctx.update_max();
        }

        if let Some(text) = &self.text {
            if !is_whitespace(text) || parent_draw_ctx.respect_whitespace {
                let text = if parent_draw_ctx.respect_whitespace {
                    text.clone()
                } else {
                    disrespect_whitespace(text)
                };
                global_ctx.draw_text(&text, style)?;
            }
        }
        for child in self.children.iter() {
            child.draw(style, global_ctx)?;
        }
        if let Some(width) = width {
            global_ctx.x += width / EM;
        }
        if let Some(height) = height {
            global_ctx.y += height / LH;
            global_ctx.x = 0;
        } else if is_display_block {
            global_ctx.y += 1;
            global_ctx.x = 0;
        }
        Ok(())
    }
}

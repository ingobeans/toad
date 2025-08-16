use std::{
    collections::HashMap,
    fmt::Debug,
    io::{self, Write},
};

use crate::{
    DEFAULT_DRAW_CTX, Display, DrawCall, ElementDrawContext, GlobalDrawContext, Measurement,
    NonInheritedField::*, consts::*, css,
};
use crossterm::{queue, style};

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
        if char.is_whitespace()
            && let Some(last_char) = last
        {
            if last_char == char {
                continue;
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
    pub fn get_content_size(
        &self,
        parent_draw_context: ElementDrawContext,
        global_ctx: &GlobalDrawContext,
    ) -> (u16, u16) {
        let style = self.get_active_style(global_ctx, parent_draw_context);
        let mut ctx = GlobalDrawContext {
            draw_calls: Vec::new(),
            width: global_ctx.width,
            height: global_ctx.height,
            x: 0,
            y: 0,
            global_style: &global_ctx.global_style.clone(),
        };
        for child in self.children.iter() {
            let _ = child.draw(style, &mut ctx);
        }
        ctx.draw_area_size()
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
    pub fn get_active_style(
        &self,
        global_ctx: &GlobalDrawContext,
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
    pub fn draw(
        &self,
        mut parent_draw_ctx: ElementDrawContext,
        global_ctx: &mut GlobalDrawContext,
    ) -> io::Result<()> {
        // construct this element's active style
        let style = self.get_active_style(global_ctx, parent_draw_ctx);

        if self.ty.stops_parsing || matches!(style.display, Specified(Display::None)) {
            return Ok(());
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
        if is_display_block
            && let Some(width) = width
            && let Some(height) = height
            && let Specified(color) = style.background_color
        {
            // draw background color over area
            global_ctx.draw_calls.push(DrawCall::Rect(
                global_ctx.x,
                global_ctx.y,
                width / EM,
                height / LH,
                color,
            ));
        }

        if let Some(text) = &self.text
            && (!is_whitespace(text) || parent_draw_ctx.respect_whitespace)
        {
            let text = if parent_draw_ctx.respect_whitespace {
                text.clone()
            } else {
                disrespect_whitespace(text)
            };
            global_ctx.draw_calls.push(DrawCall::Text(
                global_ctx.x,
                global_ctx.y,
                text.clone(),
                style,
            ));
            let mut lines = text.lines().peekable();
            while let Some(line) = lines.next() {
                global_ctx.x += line.len() as u16;
                if lines.peek().is_some() {
                    global_ctx.x = 0;
                    global_ctx.y += 1;
                }
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

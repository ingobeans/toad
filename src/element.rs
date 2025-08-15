use std::{
    collections::HashMap,
    fmt::Debug,
    io::{self, Stdout},
};

use crate::{css, Display, ElementDrawContext, GlobalDrawContext, DEFAULT_DRAW_CTX};
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
        display: Some(Display::Block),
        ..DEFAULT_DRAW_CTX
    },
    ..DEFAULT_ELEMENT_TYPE
};
pub static ELEMENT_TYPES: &[ElementType] = &[
    ElementType {
        name: "node",
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
            display: Some(Display::Block),
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
            display: Some(Display::Block),
            ..DEFAULT_DRAW_CTX
        },
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "p",
        draw_ctx: ElementDrawContext {
            display: Some(Display::Block),
            ..DEFAULT_DRAW_CTX
        },
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "span",
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "div",
        draw_ctx: ElementDrawContext {
            display: Some(Display::Block),
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
pub fn apply_draw_ctx(
    draw_ctx: ElementDrawContext,
    old_ctx: &mut ElementDrawContext,
    mut stdout: &Stdout,
) -> io::Result<()> {
    let needs_clearing = (!draw_ctx.bold && old_ctx.bold)
        || (!draw_ctx.italics && old_ctx.italics)
        || (draw_ctx.foreground_color.is_none() && old_ctx.foreground_color.is_some())
        || (draw_ctx.background_color.is_none() && old_ctx.background_color.is_some());

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
            foreground_color: draw_ctx.foreground_color,
            background_color: draw_ctx.background_color,
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
}
impl Debug for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.print_recursive(0))
    }
}
impl Element {
    pub fn new(ty: &'static ElementType) -> Self {
        let style = ty.draw_ctx;
        Self {
            ty,
            children: Vec::new(),
            attributes: HashMap::new(),
            style,
            text: None,
        }
    }
    pub fn set_attributes(&mut self, attributes: HashMap<String, String>) {
        if let Some(style) = attributes.get("style") {
            css::parse_ruleset(&style, &mut self.style);
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
    pub fn draw(
        &self,
        mut element_draw_ctx: ElementDrawContext,
        global_ctx: &mut GlobalDrawContext,
    ) -> io::Result<()> {
        if self.ty.stops_parsing {
            return Ok(());
        }
        element_draw_ctx.merge(&self.style);

        let is_display_block = if let Some(Display::Block) = self.style.display {
            true
        } else {
            false
        };

        if is_display_block && global_ctx.x != 0 {
            global_ctx.y += 1;
            global_ctx.x = 0;
        }
        if let Some(text) = &self.text {
            if !is_whitespace(text) || element_draw_ctx.respect_whitespace {
                let text = if element_draw_ctx.respect_whitespace {
                    text.clone()
                } else {
                    disrespect_whitespace(text)
                };
                global_ctx.draw_text(&text, element_draw_ctx)?;
            }
        }
        for child in self.children.iter() {
            child.draw(element_draw_ctx, global_ctx)?;
        }
        if is_display_block {
            global_ctx.y += 1;
            global_ctx.x = 0;
        }
        Ok(())
    }
}

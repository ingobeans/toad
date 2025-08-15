use std::{
    collections::HashMap,
    fmt::Debug,
    io::{self, Stdout, Write},
};

use crate::{ElementDrawContext, GlobalDrawContext, TextAlignment};
use crossterm::{cursor, queue, style};

const RED: style::Color = style::Color::Red;

#[derive(Clone, Copy, PartialEq)]
pub struct ElementType {
    pub name: &'static str,
    pub stops_parsing: bool,
    pub needs_linebreak: bool,
    /// Element that has no closing tag, such as <img>
    pub void_element: bool,
    pub draw_ctx: ElementDrawContext,
}
pub static DEFAULT_DRAW_CTX: ElementDrawContext = ElementDrawContext {
    text_align: None,
    foreground_color: None,
    bold: false,
    italics: false,
    respect_whitespace: false,
};
pub static DEFAULT_ELEMENT_TYPE: ElementType = ElementType {
    name: "unknown",
    stops_parsing: false,
    needs_linebreak: false,
    void_element: false,
    draw_ctx: DEFAULT_DRAW_CTX,
};
static H1: ElementType = ElementType {
    name: "h1",
    needs_linebreak: true,
    draw_ctx: ElementDrawContext {
        bold: true,
        foreground_color: Some(RED),
        text_align: Some(TextAlignment::Centre),
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
        needs_linebreak: true,
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
        needs_linebreak: true,
        draw_ctx: ElementDrawContext {
            respect_whitespace: true,
            ..DEFAULT_DRAW_CTX
        },
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "p",
        needs_linebreak: true,
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "span",
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "div",
        needs_linebreak: true,
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
fn apply_draw_ctx(
    draw_ctx: ElementDrawContext,
    old_ctx: &mut ElementDrawContext,
    mut stdout: &Stdout,
) -> io::Result<()> {
    let needs_clearing = (!draw_ctx.bold && old_ctx.bold)
        || (!draw_ctx.italics && old_ctx.italics)
        || (draw_ctx.foreground_color.is_none() && old_ctx.foreground_color.is_some());

    if needs_clearing {
        queue!(stdout, style::ResetColor)?;
        old_ctx.bold = false;
        old_ctx.italics = false;
        old_ctx.foreground_color = None;
    }

    if draw_ctx.bold != old_ctx.bold {
        if draw_ctx.bold {
            queue!(stdout, style::SetAttribute(style::Attribute::Bold))?
        }
    }
    if draw_ctx.italics != old_ctx.italics {
        if draw_ctx.italics {
            queue!(stdout, style::SetAttribute(style::Attribute::Italic))?
        }
    }
    if draw_ctx
        .foreground_color
        .is_some_and(|color| old_ctx.foreground_color.is_none_or(|old| old != color))
    {
        queue!(
            stdout,
            style::SetForegroundColor(draw_ctx.foreground_color.unwrap())
        )?;
    }

    old_ctx.bold = draw_ctx.bold;
    old_ctx.italics = draw_ctx.italics;
    Ok(())
}
pub struct Element {
    pub ty: &'static ElementType,
    pub children: Vec<Element>,
    pub attributes: HashMap<String, String>,
    pub text: Option<String>,
}
impl Debug for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.print_recursive(0))
    }
}
impl Element {
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
        element_draw_ctx.merge(&self.ty.draw_ctx);

        if self.ty.needs_linebreak && !global_ctx.on_newline {
            global_ctx.y += 1;
            global_ctx.x = 0;
            global_ctx.on_newline = true;
        }
        if let Some(text) = &self.text {
            if !is_whitespace(text) || element_draw_ctx.respect_whitespace {
                if global_ctx.x != global_ctx.actual_cursor_x
                    || global_ctx.y != global_ctx.actual_cursor_y
                {
                    queue!(
                        global_ctx.stdout,
                        cursor::MoveTo(global_ctx.x, global_ctx.y)
                    )?
                }
                let text = if element_draw_ctx.respect_whitespace {
                    text.clone()
                } else {
                    disrespect_whitespace(text)
                };
                apply_draw_ctx(
                    element_draw_ctx,
                    &mut global_ctx.last_draw_ctx,
                    global_ctx.stdout,
                )?;
                global_ctx.stdout.lock().write_all(text.as_bytes())?;
                let text_len = text.len() as u16;
                let lines = (text.lines().count() as u16).saturating_sub(1);
                global_ctx.x += text_len;
                global_ctx.y += lines;
                global_ctx.actual_cursor_x = global_ctx.x;
                global_ctx.actual_cursor_y = global_ctx.y;
                if text_len > 0 {
                    global_ctx.on_newline = false;
                }
            }
        }
        for child in self.children.iter() {
            child.draw(element_draw_ctx, global_ctx)?;
        }
        if self.ty.needs_linebreak {
            global_ctx.y += 1;
            global_ctx.x = 0;
            global_ctx.on_newline = true;
        }
        Ok(())
    }
}

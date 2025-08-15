use std::{
    collections::HashMap,
    fmt::Debug,
    io::{self, Write},
};

use crossterm::{cursor, queue};

use crate::{ElementDrawContext, GlobalDrawContext};

#[derive(Clone, Copy, PartialEq)]
pub struct ElementType {
    pub name: &'static str,
    pub stops_parsing: bool,
    pub needs_linebreak: bool,
    pub respect_whitespace: bool,
    /// Element that has no closing tag, such as <img>
    pub void_element: bool,
}
pub static DEFAULT_ELEMENT_TYPE: ElementType = ElementType {
    name: "unknown",
    stops_parsing: false,
    needs_linebreak: false,
    respect_whitespace: false,
    void_element: false,
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
];
pub fn get_element_type(name: &str) -> Option<&'static ElementType> {
    ELEMENT_TYPES.iter().find(|f| f.name == name)
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
                    children_text += &child.print_recursive(index + 1);
                    children_text += "\n";
                }
                children_text
            }
        };
        let padding = "\t".repeat(index);
        let mut attributes = String::new();
        for (k, v) in &self.attributes {
            attributes += &format!(" {k}=\"{v}\"");
        }
        format!(
            "\n{padding}<{}{attributes}>\n{padding}{}\n{padding}</{}>",
            self.ty.name, children_text, self.ty.name
        )
    }
    pub fn draw(
        &self,
        element_draw_ctx: ElementDrawContext,
        global_ctx: &mut GlobalDrawContext,
    ) -> io::Result<()> {
        if self.ty.stops_parsing {
            return Ok(());
        }
        if let Some(text) = &self.text {
            if global_ctx.x != global_ctx.actual_cursor_x
                || global_ctx.y != global_ctx.actual_cursor_y
            {
                queue!(
                    global_ctx.stdout,
                    cursor::MoveTo(global_ctx.x, global_ctx.y)
                )?
            }
            global_ctx.stdout.lock().write_all(text.trim().as_bytes())?;
            global_ctx.x += text.len() as u16;
            global_ctx.actual_cursor_x = global_ctx.x;
            global_ctx.actual_cursor_y = global_ctx.y;
        }
        for child in self.children.iter() {
            child.draw(element_draw_ctx, global_ctx)?;
        }
        if self.ty.needs_linebreak {
            global_ctx.y += 1;
            global_ctx.x = 0;
        }
        Ok(())
    }
}

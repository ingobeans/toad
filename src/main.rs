use std::{
    collections::VecDeque,
    io::{self, Stdout, Write, stdout},
};

use crossterm::{
    cursor::{MoveTo, MoveToColumn},
    queue,
};
use reqwest::Url;

enum ElementType {
    /// Most basic type of element. Raw text. String is contents.
    Node(String),
    P,
    Div,
    A,
    /// Any element not implemented for Toad. String is element type.
    Other(String),
}
struct Element {
    ty: ElementType,
    children: Vec<Element>,
}
impl Element {
    fn draw(
        &self,
        element_draw_ctx: ElementDrawContext,
        global_ctx: &mut GlobalDrawContext,
    ) -> io::Result<()> {
        match &self.ty {
            ElementType::Node(text) => {
                if global_ctx.x != global_ctx.actual_cursor_x
                    || global_ctx.y != global_ctx.actual_cursor_y
                {
                    queue!(global_ctx.stdout, MoveTo(global_ctx.x, global_ctx.y))?
                }
                global_ctx.stdout.lock().write_all(text.as_bytes())?;
            }
            _ => {}
        }
        for child in self.children.iter() {
            child.draw(element_draw_ctx.clone(), global_ctx)?;
        }
        Ok(())
    }
}
#[derive(Default)]
struct Webpage {
    title: Option<String>,
    url: Option<Url>,
    body: Option<Element>,
}
#[derive(Clone, Copy)]
struct ElementDrawContext {}
struct GlobalDrawContext {
    x: u16,
    y: u16,
    actual_cursor_x: u16,
    actual_cursor_y: u16,
    stdout: Stdout,
}
struct Toad {
    tabs: Vec<Webpage>,
    tab_index: usize,
}
impl Toad {
    fn draw(&self) -> io::Result<()> {
        self.draw_current_page(0, 2)
    }
    fn draw_current_page(&self, x: u16, y: u16) -> io::Result<()> {
        let Some(tab) = self.tabs.get(self.tab_index) else {
            return Ok(());
        };
        let mut ctx = GlobalDrawContext {
            x,
            y,
            actual_cursor_x: 0,
            actual_cursor_y: 0,
            stdout: stdout(),
        };
        tab.body
            .as_ref()
            .unwrap()
            .draw(ElementDrawContext {}, &mut ctx)
    }
}
fn main() -> io::Result<()> {
    let test_page = Webpage {
        body: Some(Element {
            ty: ElementType::Node(String::from("hiya")),
            children: Vec::new(),
        }),
        ..Default::default()
    };
    let toad = Toad {
        tabs: vec![test_page],
        tab_index: 0,
    };
    toad.draw()
}

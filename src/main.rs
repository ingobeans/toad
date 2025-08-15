use crossterm::{cursor, queue, terminal};
use reqwest::Url;
use std::io::{self, Stdout, stdout};

use element::*;
use parsing::*;

mod element;
mod parsing;

#[derive(Default)]
struct Webpage {
    title: Option<String>,
    url: Option<Url>,
    root: Option<Element>,
}
#[derive(Clone, Copy)]
struct ElementDrawContext {}
struct GlobalDrawContext<'a> {
    x: u16,
    y: u16,
    actual_cursor_x: u16,
    actual_cursor_y: u16,
    stdout: &'a Stdout,
}
struct Toad {
    tabs: Vec<Webpage>,
    tab_index: usize,
}
impl Toad {
    fn draw(&self) -> io::Result<()> {
        let stdout = stdout();
        self.clear_screen(&stdout)?;
        self.draw_current_page(&stdout, 0, 2)
    }
    fn clear_screen(&self, mut stdout: &Stdout) -> io::Result<()> {
        queue!(
            stdout,
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0)
        )
    }
    fn draw_current_page(&self, stdout: &Stdout, x: u16, y: u16) -> io::Result<()> {
        let Some(tab) = self.tabs.get(self.tab_index) else {
            return Ok(());
        };
        let mut ctx = GlobalDrawContext {
            x,
            y,
            actual_cursor_x: 0,
            actual_cursor_y: 0,
            stdout,
        };
        tab.root
            .as_ref()
            .unwrap()
            .draw(ElementDrawContext {}, &mut ctx)
    }
}
fn main() -> io::Result<()> {
    let test_page = parse_html(include_str!("test.html")).unwrap();
    // println!("{:?}", test_page.root);
    // Ok(())
    let toad = Toad {
        tabs: vec![test_page],
        tab_index: 0,
    };
    toad.draw()
}

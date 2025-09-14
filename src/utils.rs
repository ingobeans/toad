use std::io::{Stdout, Write, stdout};

use crossterm::{
    cursor,
    event::{self, KeyCode, KeyModifiers},
    execute, queue, style, terminal,
};
use unicode_width::UnicodeWidthStr;

pub fn pop_until<T: PartialEq>(a: &mut Vec<T>, b: &T) -> Vec<T> {
    let mut popped = Vec::new();
    while let Some(item) = a.pop() {
        if &item == b {
            return popped;
        }
        popped.push(item);
    }
    popped
}
pub fn pop_until_any<T: PartialEq>(a: &mut Vec<T>, b: &[T]) -> (Vec<T>, Option<T>) {
    let mut popped = Vec::new();
    while let Some(item) = a.pop() {
        if b.contains(&item) {
            return (popped, Some(item));
        }
        popped.push(item);
    }
    (popped, None)
}
pub fn pop_until_all<T: PartialEq>(a: &mut Vec<T>, b: &[T]) -> Vec<T> {
    let mut match_index = 0;
    let mut popped = Vec::new();
    while let Some(item) = a.pop() {
        if b[match_index] == item {
            match_index += 1;
            if match_index >= b.len() {
                return popped;
            }
            continue;
        }
        match_index = 0;
        popped.push(item);
    }
    popped
}
pub fn next_is<T: PartialEq>(a: &[T], b: &T) -> bool {
    let Some(item) = a.last() else {
        return false;
    };
    item == b
}
pub fn add_panic_handler() {
    std::panic::set_hook(Box::new(|f| {
        terminal::disable_raw_mode().unwrap();
        execute!(stdout(), cursor::Show).unwrap();
        let mut p = String::new();
        if let Some(a) = f.payload().downcast_ref::<&str>() {
            p = a.to_string();
        }
        if let Some(a) = f.payload().downcast_ref::<String>() {
            p = a.to_string();
        }
        let a = format!("TOAD panicked at: {:?}\n\nError: {:?}", f.location(), p);
        std::fs::write("error.txt", a).unwrap();
    }));
}
pub fn remove_whitespace(input: &str) -> String {
    input
        .replace(" ", "")
        .replace("\t", "")
        .replace("\n", "")
        .replace("\r", "")
}

fn insert_char(string: &mut String, insert: char, index: usize) {
    if index >= string.chars().count() {
        string.push(insert);
        return;
    }
    let mut new = String::new();
    for (i, char) in string.chars().enumerate() {
        if i == index {
            new.push(insert);
        }
        new.push(char);
    }
    *string = new;
}
fn remove_char(string: &mut String, index: usize) {
    let mut new = String::new();
    for (i, char) in string.chars().enumerate() {
        if i != index {
            new.push(char);
        }
    }
    *string = new;
}

pub enum InputBoxSubmitTarget {
    OpenNewTab,
    ChangeAddress,
    SetFormTextField(usize, String),
}

pub enum InputBoxState {
    Active,
    Submitted,
    Cancelled,
}

pub struct InputBox {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub text: String,
    cursor_pos: usize,
    pub state: InputBoxState,
    pub on_submit: InputBoxSubmitTarget,
}
impl InputBox {
    pub fn new(
        x: u16,
        y: u16,
        width: u16,
        on_submit: InputBoxSubmitTarget,
        text: Option<String>,
    ) -> Self {
        let text = text.unwrap_or_default();
        Self {
            x,
            y,
            width,
            cursor_pos: text.chars().count(),
            text,
            state: InputBoxState::Active,
            on_submit,
        }
    }
    pub fn draw(&self, mut stdout: &Stdout) -> std::io::Result<()> {
        queue!(
            stdout,
            cursor::Show,
            cursor::MoveTo(self.x, self.y),
            style::ResetColor
        )?;
        write!(
            stdout,
            "{}{}",
            self.text,
            " ".repeat((self.width as usize).saturating_sub(self.text.width()))
        )?;
        queue!(
            stdout,
            cursor::MoveToColumn(self.x + self.cursor_pos as u16)
        )?;
        Ok(())
    }
    pub fn on_event(&mut self, event: event::KeyEvent) {
        match event.code {
            KeyCode::Left => {
                self.cursor_pos = self.cursor_pos.saturating_sub(1);
            }
            KeyCode::Right => {
                self.cursor_pos += 1;
                if self.cursor_pos > self.text.chars().count() {
                    self.cursor_pos -= 1;
                }
            }
            KeyCode::Enter => {
                self.state = InputBoxState::Submitted;
            }
            KeyCode::Esc => {
                self.state = InputBoxState::Cancelled;
            }
            KeyCode::Char(char) => {
                if char == 'c' && event.modifiers.contains(KeyModifiers::CONTROL) {
                    self.state = InputBoxState::Cancelled;
                } else {
                    insert_char(&mut self.text, char, self.cursor_pos);
                    self.cursor_pos += 1;
                }
            }
            KeyCode::Backspace => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                    remove_char(&mut self.text, self.cursor_pos);
                }
            }
            KeyCode::Delete => {
                remove_char(&mut self.text, self.cursor_pos);
            }
            _ => {}
        }
    }
}

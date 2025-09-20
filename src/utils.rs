use std::{
    collections::VecDeque,
    io::{Stdout, Write, stdout},
};

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
        let a = format!(
            r"  _______ ____          _____  
 |__   __/ __ \   /\   |  __ \ 
    | | | |  | | /  \  | |  | |
    | | | |  | |/ /\ \ | |  | |
    | | | |__| / ____ \| |__| |
    |_|  \____/_/    \_\_____/ 

CRASHREPORT - sorry about this :<

Panic at: {:?}

Error: {:?}",
            f.location(),
            p
        );
        let path = if let Ok(p) = std::env::current_exe()
            && let Some(d) = p.parent()
        {
            d.join("error_log.txt")
        } else {
            "error_log.txt".into()
        };
        std::fs::write(path, a).unwrap();
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

pub const SPECIAL_CHARS: &[char] = &['.', '/', ' '];

pub struct InputBox {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub text: String,
    cursor_pos: usize,
    pub state: InputBoxState,
    pub on_submit: InputBoxSubmitTarget,
    auto_completions: Vec<String>,
    rejected_autocompletion: bool,
}
impl InputBox {
    pub fn new(
        x: u16,
        y: u16,
        width: u16,
        on_submit: InputBoxSubmitTarget,
        text: Option<String>,
        auto_completions: Vec<String>,
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
            auto_completions,
            rejected_autocompletion: false,
        }
    }
    fn get_autocompletion(&self) -> Option<String> {
        if self.rejected_autocompletion {
            return None;
        }
        self.auto_completions.iter().find_map(|f| {
            if !self.text.is_empty() && f.starts_with(&self.text) {
                let text_chars = self.text.chars().count();
                let mut chars: VecDeque<char> = f.clone().chars().collect();
                if text_chars >= chars.len() {
                    return None;
                }

                for _ in 0..text_chars {
                    chars.pop_front();
                }
                Some(chars.iter().collect::<String>())
            } else {
                None
            }
        })
    }
    pub fn draw(&self, mut stdout: &Stdout) -> std::io::Result<()> {
        queue!(
            stdout,
            cursor::Show,
            cursor::MoveTo(self.x, self.y),
            style::ResetColor
        )?;
        let autocomplete = self.get_autocompletion().unwrap_or_default();
        write!(stdout, "{}", self.text)?;
        queue!(stdout, style::SetBackgroundColor(style::Color::Blue))?;
        write!(stdout, "{autocomplete}")?;
        queue!(stdout, style::ResetColor)?;
        write!(
            stdout,
            "{}",
            " ".repeat(
                (self.width as usize).saturating_sub(self.text.width() + autocomplete.width())
            )
        )?;
        queue!(
            stdout,
            cursor::MoveToColumn(self.x + self.cursor_pos as u16)
        )?;
        Ok(())
    }
    pub fn on_event(&mut self, event: event::KeyEvent) {
        let mut realize_autocompletion = false;
        let mut jump_to_autocompletion_end = false;
        let autocompletion = self.get_autocompletion();
        match event.code {
            KeyCode::Left => {
                self.cursor_pos = self.cursor_pos.saturating_sub(1);

                if event.modifiers.contains(event::KeyModifiers::CONTROL) {
                    let chars: Vec<char> = self.text.chars().collect();
                    while self.cursor_pos > 0 && !SPECIAL_CHARS.contains(&chars[self.cursor_pos]) {
                        self.cursor_pos -= 1;
                    }
                }
                realize_autocompletion = true;
            }
            KeyCode::Right => {
                self.cursor_pos += 1;
                if self.cursor_pos > self.text.chars().count() {
                    self.cursor_pos -= 1;
                }
                if event.modifiers.contains(event::KeyModifiers::CONTROL) {
                    let chars: Vec<char> = self.text.chars().collect();
                    while self.cursor_pos < self.text.chars().count()
                        && !SPECIAL_CHARS.contains(&chars[self.cursor_pos])
                    {
                        self.cursor_pos += 1;
                    }
                }
                jump_to_autocompletion_end = true;
                realize_autocompletion = true;
            }
            KeyCode::Enter => {
                self.state = InputBoxState::Submitted;
                realize_autocompletion = true;
            }
            KeyCode::Esc => {
                self.state = InputBoxState::Cancelled;
            }
            KeyCode::Char(char) => {
                self.rejected_autocompletion = false;
                if char == 'c' && event.modifiers.contains(KeyModifiers::CONTROL) {
                    self.state = InputBoxState::Cancelled;
                } else {
                    insert_char(&mut self.text, char, self.cursor_pos);
                    self.cursor_pos += 1;
                }
            }
            KeyCode::Home => {
                self.cursor_pos = 0;
                realize_autocompletion = true;
            }
            KeyCode::End => {
                self.cursor_pos = self.text.chars().count();
                realize_autocompletion = true;
                jump_to_autocompletion_end = true;
            }
            KeyCode::Backspace => {
                if self.cursor_pos > 0 {
                    self.rejected_autocompletion = true;
                    if autocompletion.is_none() {
                        self.cursor_pos -= 1;
                        remove_char(&mut self.text, self.cursor_pos);

                        // make ctrl+backspace delete until special character
                        //
                        // note: if using vscode to test, ctrl+backspace doesnt work in vscode's terminal
                        // so you'll have to use another terminal
                        if event.modifiers.contains(event::KeyModifiers::CONTROL) {
                            let mut chars: Vec<char> = self.text.chars().collect();
                            while self.cursor_pos > 0
                                && !SPECIAL_CHARS.contains(&chars[self.cursor_pos - 1])
                            {
                                self.cursor_pos -= 1;
                                chars.remove(self.cursor_pos);
                            }
                            self.text = chars.iter().collect();
                        }
                    }
                }
            }
            KeyCode::Delete => {
                self.rejected_autocompletion = true;
                if autocompletion.is_none() {
                    remove_char(&mut self.text, self.cursor_pos);

                    // make ctrl+delete delete until special character
                    //
                    // again, ctrl+delete doesnt work in vscode's terminal
                    // so this has to be tested in another terminal
                    if event.modifiers.contains(event::KeyModifiers::CONTROL) {
                        let mut chars: Vec<char> = self.text.chars().collect();
                        while self.cursor_pos < chars.len()
                            && !SPECIAL_CHARS.contains(&chars[self.cursor_pos])
                        {
                            chars.remove(self.cursor_pos);
                        }
                        self.text = chars.iter().collect();
                    }
                }
            }
            _ => {}
        }
        if realize_autocompletion && let Some(autocompletion) = autocompletion {
            self.text += &autocompletion;
            if jump_to_autocompletion_end {
                self.cursor_pos = self.text.chars().count();
            }
        }
    }
}

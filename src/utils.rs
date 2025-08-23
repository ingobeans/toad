use std::io::{self, Write, stdout};

use crossterm::{cursor, execute, queue, terminal};

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
pub fn get_line_input<T: Write>(stdout: &mut T, x: u16, y: u16) -> io::Result<String> {
    terminal::disable_raw_mode()?;
    execute!(
        stdout,
        cursor::MoveTo(x, y),
        terminal::Clear(terminal::ClearType::CurrentLine),
        cursor::Show
    )?;
    let mut buf = String::new();
    io::stdin().read_line(&mut buf)?;
    terminal::enable_raw_mode()?;
    queue!(stdout, cursor::Hide)?;
    Ok(buf.trim().to_string())
}

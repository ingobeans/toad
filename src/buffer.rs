use std::{
    borrow::Cow,
    io::{self, Write},
};

use crossterm::{
    cursor, queue,
    style::{self, Color},
};
use image::{DynamicImage, GenericImageView};
use unicode_width::UnicodeWidthChar;

use crate::{ElementDrawContext, NonInheritedField, consts::*};

#[derive(Clone, Copy)]
struct Cell {
    char: char,
    foreground_color: Color,
    background_color: Color,
    bold: bool,
    italics: bool,
}
impl Cell {
    fn compare_style(&self, other: &Cell) -> bool {
        self.foreground_color == other.foreground_color
            && self.background_color == other.background_color
            && self.bold == other.bold
            && self.italics == other.italics
    }
    fn format_stdout<T: Write>(&self, stdout: &mut T, last: &mut Cell) -> io::Result<()> {
        if self.compare_style(last) {
            return Ok(());
        }
        let needs_clearing = (!self.bold && last.bold) || (!self.italics && last.italics);

        if needs_clearing {
            queue!(stdout, style::ResetColor)?;
        }
        let mut attributes = style::Attributes::none();

        if self.bold {
            attributes.set(style::Attribute::Bold);
        }
        if self.italics {
            attributes.set(style::Attribute::Italic);
        }

        queue!(
            stdout,
            style::SetStyle(style::ContentStyle {
                foreground_color: Some(self.foreground_color),
                background_color: Some(self.background_color),
                attributes,
                ..Default::default()
            })
        )?;

        last.bold = self.bold;
        last.italics = self.italics;
        last.foreground_color = self.foreground_color;
        last.background_color = self.background_color;
        Ok(())
    }
}
impl Default for Cell {
    fn default() -> Self {
        Self {
            char: ' ',
            foreground_color: BLACK_COLOR,
            background_color: WHITE_COLOR,
            bold: false,
            italics: false,
        }
    }
}

fn apply_draw_ctx_to_cell(draw_ctx: &ElementDrawContext, cell: &mut Cell) {
    // always apply foreground color
    cell.foreground_color = draw_ctx.foreground_color.unwrap_or(BLACK_COLOR);
    // background color doesnt have to be applied, and will use whatever was there previously
    if let NonInheritedField::Specified(background_color) = draw_ctx.background_color {
        cell.background_color = background_color;
    }
    // always apply
    cell.bold = draw_ctx.bold;
    cell.italics = draw_ctx.italics;
}

/// Convert rgba value \[u8;4\] to a [crossterm color](crossterm::style::Color)
fn rgba_to_color(rgba: [u8; 4]) -> crossterm::style::Color {
    crossterm::style::Color::Rgb {
        r: rgba[0],
        g: rgba[1],
        b: rgba[2],
    }
}

pub struct Buffer {
    data: Vec<Cell>,
    pub interactables: Vec<Option<usize>>,
    width: usize,
    height: usize,
}
impl Buffer {
    pub fn empty(width: u16, height: u16) -> Self {
        Self {
            data: vec![Cell::default(); width as usize * height as usize],
            interactables: vec![None; width as usize * height as usize],
            width: width as _,
            height: height as _,
        }
    }
    pub fn clear_color(&mut self, color: Color) {
        let cell = Cell {
            background_color: color,
            ..Default::default()
        };
        self.data = vec![cell; self.width * self.height]
    }
    pub fn render<T: Write>(
        &self,
        stdout: &mut T,
        prev: Option<&Buffer>,
        start_x: usize,
        start_y: usize,
    ) -> io::Result<()> {
        let mut last = Cell::default();
        last.background_color = Color::Reset;
        let mut data = self.data.iter().enumerate();
        let mut prev_data = prev.map(|f| f.data.iter());

        let mut cursor_x = start_x;
        let mut cursor_y = start_y;

        while let Some((index, cell)) = data.next() {
            if let Some(ref mut prev) = prev_data
                && let Some(prev) = prev.next()
                && prev.compare_style(cell)
                && prev.char == cell.char
            {
                continue;
            }
            let x = index % self.width + start_x;
            let y = index / self.width + start_y;

            if cursor_x != x || cursor_y != y {
                queue!(stdout, cursor::MoveTo(x as u16, y as u16))?;
                cursor_x = x;
                cursor_y = y;
            }

            let char = cell.char;
            assert_ne!(char, '\n');
            cell.format_stdout(stdout, &mut last)?;
            write!(stdout, "{}", char)?;
            let width = char.width().unwrap_or_default();

            cursor_x += width;

            last = *cell;
            if width > 1 {
                // skip next
                data.next();
                if let Some(ref mut prev) = prev_data {
                    prev.next();
                }
            }
        }
        Ok(())
    }
    pub fn set_pixel(&mut self, x: u16, y: u16, color: Color) {
        self.data[x as usize + y as usize * self.width] = Cell {
            background_color: color,
            ..Default::default()
        };
    }
    pub fn get_interactable(&self, x: usize, y: usize) -> Option<usize> {
        if y >= self.height || x >= self.width {
            None
        } else {
            self.interactables[y * self.width + x]
        }
    }
    #[expect(clippy::too_many_arguments)]
    pub fn draw_input_box(
        &mut self,
        x: u16,
        y: u16,
        row: u16,
        width: u16,
        height: u16,
        text: &str,
        highlighted: bool,
        interactable: usize,
    ) {
        let mut text_chars = text.chars();
        let background_color = if !highlighted { GREY_COLOR } else { BLUE_COLOR };
        let border_color = BLACK_COLOR;
        let mut skip = false;
        for column in 0..width {
            if skip {
                skip = false;
                continue;
            }
            let index = column as usize + x as usize + y as usize * self.width;
            let char: Cow<str> = if row == 0 {
                if column == 0 {
                    Cow::Borrowed(box_drawing::double::DOWN_RIGHT)
                } else if column == width - 1 {
                    Cow::Borrowed(box_drawing::double::DOWN_LEFT)
                } else {
                    Cow::Borrowed(box_drawing::double::HORIZONTAL)
                }
            } else if row == height - 1 {
                if column == 0 {
                    Cow::Borrowed(box_drawing::double::UP_RIGHT)
                } else if column == width - 1 {
                    Cow::Borrowed(box_drawing::double::UP_LEFT)
                } else {
                    Cow::Borrowed(box_drawing::double::HORIZONTAL)
                }
            } else if column == 0 || column == width - 1 {
                Cow::Borrowed(box_drawing::double::VERTICAL)
            } else if row == 1
                && let Some(char) = text_chars.next()
            {
                if column < width - 3 {
                    Cow::Owned(char.to_string())
                } else {
                    Cow::Borrowed(".")
                }
            } else {
                Cow::Borrowed(" ")
            };
            let char = char.chars().next().unwrap();

            let cell = Cell {
                char,
                background_color,
                foreground_color: border_color,
                ..Default::default()
            };
            self.data[index] = cell;
            self.interactables[index] = Some(interactable);
            if let Some(w) = char.width()
                && w > 1
            {
                self.data[index + 1] = Cell { char: ' ', ..cell };
                self.interactables[index + 1] = Some(interactable);
                skip = true;
            }
        }
    }
    pub fn draw_img_row(&mut self, x: u16, y: u16, row: u32, image: &DynamicImage) {
        for column in 0..image.width() {
            let index = column as usize + x as usize + y as usize * self.width;
            let background_color = self.data[index].background_color;
            let top_rgba = image.get_pixel(column as _, row as _).0;
            let top_color = if top_rgba[3] == 0 {
                background_color
            } else {
                rgba_to_color(top_rgba)
            };

            let bottom_color = if row == image.height() {
                // if at last row, pretend bottom pixel is background
                background_color
            } else {
                // if not at last row, read rgba of below pixel
                let rgb = image.get_pixel(column, row + 1).0;
                if rgb[3] == 0 {
                    background_color
                } else {
                    rgba_to_color(rgb)
                }
            };

            let cell = Cell {
                char: '▀',
                background_color: bottom_color,
                foreground_color: top_color,
                ..Default::default()
            };
            self.data[index] = cell;
        }
    }
    pub fn draw_rect(&mut self, x: u16, y: u16, width: u16, height: u16, color: Color) {
        let (x, y, width, height) = (x as usize, y as usize, width as usize, height as usize);
        for i in 0..height {
            for j in 0..width {
                if y + i >= self.height {
                    continue;
                }
                if x + j >= self.width {
                    continue;
                }
                let index = x + j + (y + i) * self.width;
                let cell = self.data.get_mut(index).unwrap();
                cell.char = ' ';
                cell.background_color = color;
            }
        }
    }
    /// Insert a string somewhere. Newlines not permitted!
    pub fn draw_str(
        &mut self,
        x: u16,
        y: u16,
        text: &str,
        draw_ctx: &ElementDrawContext,
        interactable: Option<usize>,
    ) {
        let y = y as usize;
        if y >= self.height {
            return;
        }
        let mut x = x as usize;
        for char in text.chars() {
            if x >= self.width {
                continue;
            }
            let width = char.width().unwrap_or_default();
            let i = x + y * self.width;
            let cell = self.data.get_mut(i).unwrap();
            self.interactables[i] = interactable;
            cell.char = char;
            apply_draw_ctx_to_cell(draw_ctx, cell);

            // if double width char, make next char empty
            if width > 1 {
                let i = i + 1;
                let cell = self.data.get_mut(i).unwrap();
                cell.char = ' ';
                apply_draw_ctx_to_cell(draw_ctx, cell);
                self.interactables[i] = interactable;
            }
            x += width;
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::style::Color;

    use crate::{DEFAULT_DRAW_CTX, buffer::Buffer, consts::BLUE_COLOR};

    #[test]
    fn test_write_str() {
        let mut buf = Buffer::empty(10, 2);
        let text = "hello";
        buf.draw_str(0, 0, text, &DEFAULT_DRAW_CTX, None);
        for (index, char) in text.chars().enumerate() {
            assert_eq!(buf.data[index].char, char)
        }
    }
    #[test]
    fn test_wide_chars() {
        let mut buf = Buffer::empty(10, 2);
        buf.draw_str(0, 0, "aaaaaaaa", &DEFAULT_DRAW_CTX, None);
        assert_eq!(buf.data[1].char, 'a');
        let text = "🍌";
        buf.draw_str(0, 0, text, &DEFAULT_DRAW_CTX, None);
        assert_eq!(buf.data[1].char, ' ');
    }
    #[test]
    fn test_rect() {
        let mut buf = Buffer::empty(10, 2);
        buf.draw_rect(1, 0, 5, 1, BLUE_COLOR);
        assert_eq!(buf.data[0].background_color, Color::Reset);
        assert_eq!(buf.data[1].background_color, BLUE_COLOR);
    }
}

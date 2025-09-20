use crossterm::style;

use crate::Theme;

pub const EM: u16 = 8;
pub const LH: u16 = 16;

pub static LIGHT_THEME: Theme = Theme {
    background_color: style::Color::Rgb {
        r: 255,
        g: 255,
        b: 255,
    },
    text_color: style::Color::Rgb { r: 0, g: 0, b: 0 },
    ui_color: style::Color::Rgb {
        r: 174,
        g: 175,
        b: 204,
    },
    interactive_color: style::Color::Rgb {
        r: 129,
        g: 154,
        b: 255,
    },
    is_dark: false,
};

pub static DARK_THEME: Theme = Theme {
    background_color: style::Color::Rgb {
        r: 55,
        g: 55,
        b: 55,
    },
    text_color: style::Color::Rgb {
        r: 255,
        g: 255,
        b: 255,
    },
    ui_color: style::Color::Rgb { r: 0, g: 0, b: 0 },
    interactive_color: style::Color::Rgb {
        r: 15,
        g: 54,
        b: 189,
    },
    is_dark: true,
};

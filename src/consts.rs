use crossterm::style;

pub const EM: u16 = 8;
pub const LH: u16 = 16;

pub const BLACK_COLOR: style::Color = style::Color::Rgb { r: 0, g: 0, b: 0 };
pub const WHITE_COLOR: style::Color = style::Color::Rgb {
    r: 255,
    g: 255,
    b: 255,
};
pub const GREY_COLOR: style::Color = style::Color::Rgb {
    r: 174,
    g: 175,
    b: 204,
};
pub const BLUE_COLOR: style::Color = style::Color::Rgb {
    r: 129,
    g: 154,
    b: 255,
};

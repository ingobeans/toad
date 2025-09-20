use crossterm::style;

use crate::consts::*;

pub fn write_settings(settings: &ToadSettings) {
    let path = if let Ok(p) = std::env::current_exe()
        && let Some(d) = p.parent()
    {
        d.join(CONFIG_FILENAME)
    } else {
        return;
    };
    let _ = std::fs::write(path, settings.serialize());
}
pub fn load_settings() -> ToadSettings {
    let path = if let Ok(p) = std::env::current_exe()
        && let Some(d) = p.parent()
    {
        d.join(CONFIG_FILENAME)
    } else {
        return ToadSettings::default();
    };
    if path.exists() {
        if let Ok(data) = std::fs::read(path) {
            return ToadSettings::deserialize(&data);
        }
    }
    ToadSettings::default()
}

pub struct Theme {
    /// White on light theme
    pub background_color: style::Color,
    /// Black on light theme
    pub text_color: style::Color,
    /// Grey on light theme
    pub ui_color: style::Color,
    /// Blue on light theme
    pub interactive_color: style::Color,
    /// False on light theme
    ///
    /// Used for CSS media selectors
    pub is_dark: bool,
}
pub struct ToadSettings {
    pub images_enabled: bool,
    pub theme: &'static Theme,
}
impl ToadSettings {
    pub fn serialize(&self) -> Vec<u8> {
        let mut data = Vec::new();
        data.push(if self.images_enabled { 1 } else { 0 });
        data.push(
            THEMES
                .iter()
                .position(|f| std::ptr::eq(f, self.theme))
                .unwrap() as u8,
        );
        data
    }
    pub fn deserialize(data: &[u8]) -> Self {
        let images_enabled = data[0] == 1;
        let theme_index = data[1] as usize;
        Self {
            images_enabled,
            theme: &THEMES[theme_index],
        }
    }
}
impl Default for ToadSettings {
    fn default() -> Self {
        Self {
            images_enabled: true,
            theme: &THEMES[0],
        }
    }
}

pub static THEMES: &[Theme] = &[
    Theme {
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
    },
    Theme {
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
            r: 192,
            g: 212,
            b: 255,
        },
        is_dark: true,
    },
];

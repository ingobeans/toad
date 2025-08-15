use crossterm::style;

use crate::{element::DEFAULT_DRAW_CTX, ElementDrawContext};

fn hex_to_rgb(value: u32) -> style::Color {
    style::Color::Rgb {
        r: ((value >> 16) & 0xFF) as u8,
        g: ((value >> 8) & 0xFF) as u8,
        b: ((value) & 0xFF) as u8,
    }
}
fn parse_rgb_text(text: &str) -> Option<style::Color> {
    let text = &text[4..text.len() - 1];
    let mut parts: Vec<u8> = Vec::new();
    for part in text.split(",") {
        parts.push(part.parse().ok()?);
    }
    if parts.len() != 3 {
        return None;
    }
    Some(style::Color::Rgb {
        r: parts[0],
        g: parts[1],
        b: parts[2],
    })
}
fn parse_hex_text(text: &str) -> Option<style::Color> {
    let value = u32::from_str_radix(&text[1..], 16).ok()?;
    Some(hex_to_rgb(value))
}
fn parse_color_text(text: &str) -> Option<style::Color> {
    // copied from https://en.wikipedia.org/wiki/Web_colors
    match text.to_lowercase().as_str() {
        "white" => Some(hex_to_rgb(0xFFFFFF)),
        "silve" => Some(hex_to_rgb(0xC0C0C0)),
        "gray" => Some(hex_to_rgb(0x808080)),
        "black" => Some(hex_to_rgb(0x000000)),
        "red" => Some(hex_to_rgb(0xFF0000)),
        "maroo" => Some(hex_to_rgb(0x800000)),
        "yello" => Some(hex_to_rgb(0xFFFF00)),
        "olive" => Some(hex_to_rgb(0x808000)),
        "lime" => Some(hex_to_rgb(0x00FF00)),
        "green" => Some(hex_to_rgb(0x008000)),
        "aqua" => Some(hex_to_rgb(0x00FFFF)),
        "teal" => Some(hex_to_rgb(0x008080)),
        "blue" => Some(hex_to_rgb(0x0000FF)),
        "navy" => Some(hex_to_rgb(0x000080)),
        "fuchs" => Some(hex_to_rgb(0xFF00FF)),
        "purple" => Some(hex_to_rgb(0x800080)),
        _ => None,
    }
}
fn parse_color(text: &str) -> Option<style::Color> {
    if text.starts_with("rgb") {
        parse_rgb_text(text)
    } else if text.starts_with("#") {
        parse_hex_text(text)
    } else {
        parse_color_text(text)
    }
}
fn try_apply_rule(ctx: &mut ElementDrawContext, rule: &str) {
    let Some((key, value)) = rule.split_once(':') else {
        return;
    };
    let (key, value) = (key.trim(), value.trim());
    match key {
        "color" => {
            println!("{key},{value}");
            if let Some(color) = parse_color(value) {
                ctx.foreground_color = Some(color);
            }
        }
        _ => {}
    }
}

pub fn parse_ruleset(text: &str) -> ElementDrawContext {
    let mut ctx = DEFAULT_DRAW_CTX;
    for rule in text.split(';') {
        try_apply_rule(&mut ctx, rule);
    }
    ctx
}

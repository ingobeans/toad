use crossterm::style;

use crate::{Display, ElementDrawContext, TextAlignment, DEFAULT_DRAW_CTX};

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
        parts.push(part.trim().parse().ok()?);
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
fn parse_align_mode(text: &str) -> Option<TextAlignment> {
    match text.to_lowercase().trim() {
        "center" => Some(TextAlignment::Centre),
        "left" | "start" => Some(TextAlignment::Left),
        "right" | "end" => Some(TextAlignment::Right),
        _ => None,
    }
}
fn parse_display_mode(text: &str) -> Option<Display> {
    match text.to_lowercase().trim() {
        "block" => Some(Display::Block),
        "inline" => Some(Display::Inline),
        _ => None,
    }
}
fn try_apply_rule(ctx: &mut ElementDrawContext, rule: &str) {
    let Some((key, value)) = rule.split_once(':') else {
        return;
    };
    let (key, value) = (key.trim(), value.trim());
    match key {
        "color" => {
            if let Some(color) = parse_color(value) {
                ctx.foreground_color = Some(color);
            }
        }
        "background-color" => {
            if let Some(color) = parse_color(value) {
                ctx.background_color = Some(color);
            }
        }
        "text-align" => {
            if let Some(align_mode) = parse_align_mode(value) {
                ctx.text_align = Some(align_mode);
            }
        }
        "display" => {
            if let Some(display_mode) = parse_display_mode(value) {
                ctx.display = Some(display_mode);
            }
        }
        _ => {}
    }
}

pub fn parse_ruleset(text: &str, ctx: &mut ElementDrawContext) {
    for rule in text.split(';') {
        try_apply_rule(ctx, rule);
    }
}

use std::collections::HashMap;

use crossterm::style;

use crate::{
    DEFAULT_DRAW_CTX, Display, ElementDrawContext, Measurement, NonInheritedField::*, StyleTarget,
    TextAlignment, consts::*, utils::*,
};

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
        "silver" => Some(hex_to_rgb(0xC0C0C0)),
        "gray" => Some(hex_to_rgb(0x808080)),
        "black" => Some(hex_to_rgb(0x000000)),
        "red" => Some(hex_to_rgb(0xFF0000)),
        "maroon" => Some(hex_to_rgb(0x800000)),
        "yellow" => Some(hex_to_rgb(0xFFFF00)),
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
        "none" => Some(Display::None),
        _ => None,
    }
}
fn parse_measurement(text: &str) -> Option<Measurement> {
    if text.ends_with("px") {
        text.trim_end_matches("px")
            .parse::<u16>()
            .ok()
            .map(Measurement::Pixels)
    } else if text.ends_with("em") {
        text.trim_end_matches("em")
            .parse::<u16>()
            .ok()
            .map(|f| Measurement::Pixels(f * EM))
    } else if text.ends_with("lh") {
        text.trim_end_matches("lh")
            .parse::<u16>()
            .ok()
            .map(|f| Measurement::Pixels(f * LH))
    } else {
        None
    }
}
fn parse_horizontal_measurement(text: &str) -> Option<Measurement> {
    if text.ends_with("%") {
        text.trim_end_matches("%")
            .parse::<f32>()
            .ok()
            .map(|f| Measurement::PercentWidth(f / 100.0))
    } else {
        parse_measurement(text)
    }
}
fn parse_vertical_measurement(text: &str) -> Option<Measurement> {
    if text.ends_with("%") {
        text.trim_end_matches("%")
            .parse::<f32>()
            .ok()
            .map(|f| Measurement::PercentHeight(f / 100.0))
    } else {
        parse_measurement(text)
    }
}
fn parse_width(text: &str) -> Option<Measurement> {
    if text == "fit-content" {
        Some(Measurement::FitContentWidth)
    } else {
        parse_horizontal_measurement(text)
    }
}

fn parse_height(text: &str) -> Option<Measurement> {
    if text == "fit-content" {
        Some(Measurement::FitContentHeight)
    } else {
        parse_vertical_measurement(text)
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
            if value == "inherit" {
                ctx.background_color = Inherit;
            } else if let Some(color) = parse_color(value) {
                ctx.background_color = Specified(color);
            }
        }
        "text-align" => {
            if let Some(align_mode) = parse_align_mode(value) {
                ctx.text_align = Some(align_mode);
            }
        }
        "display" => {
            if value == "inherit" {
                ctx.display = Inherit;
            } else if let Some(display_mode) = parse_display_mode(value) {
                ctx.display = Specified(display_mode);
            }
        }
        "width" => {
            if let Some(width) = parse_width(value) {
                ctx.width = Some(width);
            }
        }
        "height" => {
            if value == "inherit" {
                ctx.height = Inherit;
            } else if let Some(height) = parse_height(value) {
                ctx.height = Specified(height);
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

fn pop_until_outside(text: &mut Vec<char>) {
    let mut count = 0;
    while let Some(popped) = text.pop() {
        match popped {
            '{' => count += 1,
            '}' => {
                count -= 1;
                if count == 0 {
                    return;
                }
            }
            _ => {}
        }
    }
}

pub fn parse_stylesheet(text: &str, style: &mut HashMap<StyleTarget, ElementDrawContext>) {
    let mut chars: Vec<char> = text.chars().collect();
    chars.reverse();
    while let Some(char) = chars.pop() {
        if char.is_whitespace() {
            continue;
        }
        if char == '@' {
            pop_until_outside(&mut chars);
            continue;
        }

        chars.push(char);
        let specifiers: String = pop_until(&mut chars, &'{').iter().collect();
        let data: String = pop_until(&mut chars, &'}').iter().collect();
        let mut ctx = DEFAULT_DRAW_CTX;
        parse_ruleset(&data, &mut ctx);

        for specifier in specifiers.split(",") {
            let specifier = specifier.trim();
            let Some(char) = specifier.chars().next() else {
                continue;
            };
            let target = if char == '#' {
                StyleTarget::Id(specifier[1..].to_string())
            } else if char == '.' {
                StyleTarget::Class(specifier[1..].to_string())
            } else {
                StyleTarget::ElementType(specifier.to_string())
            };
            if let Some(old) = style.get_mut(&target) {
                old.merge_all(&ctx);
            } else {
                style.insert(target, ctx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::css::pop_until_outside;

    #[test]
    fn test_pop_until_outside() {
        let mut chars: Vec<char> = "@wahoo { h { rgr grg} wello {w aw a wa} }hello {wa}"
            .chars()
            .collect();
        chars.reverse();
        pop_until_outside(&mut chars);
        chars.reverse();
        for char in &chars {
            print!("{char}");
        }
        println!("");
        assert_eq!("hello {wa}".chars().collect::<Vec<char>>(), chars)
    }
}

use crossterm::style;

use crate::{
    DEFAULT_DRAW_CTX, Display, ElementDrawContext, Measurement, NonInheritedField::*, StyleTarget,
    StyleTargetType, TextAlignment, consts::*, utils::*,
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
    let text = &text[1..];

    // hex color codes in css can be either 6 characters long, or 3.
    //
    // if it is the shorthand, each character is repeated once, such that #10f becomes #1100ff
    if text.len() == 6 {
        // for 6 character hex codes
        let value = u32::from_str_radix(text, 16).ok()?;
        Some(hex_to_rgb(value))
    } else if text.len() == 3 {
        // for 3 char hex codes
        let mut chars = text.chars();
        let a = chars.next()?;
        let b = chars.next()?;
        let c = chars.next()?;
        let text = format!("{a}{a}{b}{b}{c}{c}");
        let value = u32::from_str_radix(&text, 16).ok()?;
        Some(hex_to_rgb(value))
    } else {
        None
    }
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
        "aqua" | "cyan" => Some(hex_to_rgb(0x00FFFF)),
        "teal" => Some(hex_to_rgb(0x008080)),
        "blue" => Some(hex_to_rgb(0x0000FF)),
        "navy" => Some(hex_to_rgb(0x000080)),
        "fuchs" => Some(hex_to_rgb(0xFF00FF)),
        "purple" => Some(hex_to_rgb(0x800080)),
        _ => None,
    }
}
pub fn parse_color(text: &str) -> Option<style::Color> {
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
        "background-color" | "background" => {
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
            if value == "inherit" {
                ctx.width = Inherit;
            } else if let Some(width) = parse_width(value) {
                ctx.width = Specified(width);
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

/// Pops chars from the buf until it has exited all selectors, i.e. when the amount of opened curly braces - the amount of closed curly braces == 0.
///
/// Returns what string came before the first curly bracket, and the rest of the content.
fn pop_exit_media_selector(text: &mut Vec<char>) -> (String, String) {
    let mut selector = String::new();
    let mut content = String::new();
    let mut in_start = true;
    let mut count = 0;
    while let Some(popped) = text.pop() {
        match popped {
            '{' => {
                if in_start {
                    in_start = false;
                } else {
                    content.push(popped);
                }
                count += 1
            }
            '}' => {
                count -= 1;
                if count == 0 {
                    break;
                }
                content.push(popped);
            }
            _ => {
                if in_start {
                    selector.push(popped);
                } else {
                    content.push(popped);
                }
            }
        }
    }
    (selector, content)
}
fn parse_target_type(specifier: &str, type_requirement: Option<String>) -> Option<StyleTargetType> {
    let char = specifier.chars().next()?;
    let target: StyleTargetType = if char == '#' {
        StyleTargetType::Id(specifier[1..].to_string(), type_requirement)
    } else if char == '.' {
        StyleTargetType::Class(specifier[1..].to_string(), type_requirement)
    } else if specifier.contains(".") || specifier.contains("#") {
        let ty: &str;
        let new: String;
        if specifier.contains(".") {
            let s;
            (ty, s) = specifier.split_once(".")?;
            new = format!(".{s}");
        } else {
            let s;
            (ty, s) = specifier.split_once("#")?;
            new = format!("#{s}");
        }
        parse_target_type(&new, Some(ty.to_string()))?
    } else {
        StyleTargetType::ElementType(specifier.to_string())
    };
    Some(target)
}
fn parse_target(specifier: &str) -> Option<StyleTarget> {
    let mut types = Vec::new();
    for item in specifier.split(" ") {
        if item.is_empty() {
            continue;
        }
        if let Some(target_type) = parse_target_type(item, None) {
            types.push(target_type);
        }
    }
    if types.is_empty() {
        None
    } else {
        Some(StyleTarget { types })
    }
}
pub fn parse_stylesheet(text: &str, style: &mut Vec<(StyleTarget, ElementDrawContext)>) {
    let mut chars: Vec<char> = text.chars().collect();
    chars.reverse();
    while let Some(char) = chars.pop() {
        if char.is_whitespace() {
            continue;
        }
        if char == '@' {
            let (media_selector, rule_contents) = pop_exit_media_selector(&mut chars);
            // also parse the content of the media selector thingy
            if media_selector.trim() == "media screen" {
                parse_stylesheet(&rule_contents, style);
            }
            continue;
        }

        chars.push(char);
        let specifiers: String = pop_until(&mut chars, &'{').iter().collect();
        let data: String = pop_until(&mut chars, &'}').iter().collect();
        let mut ctx = DEFAULT_DRAW_CTX;
        parse_ruleset(&data, &mut ctx);

        for specifier in specifiers.split(",") {
            let specifier = specifier.trim();
            let Some(target) = parse_target(specifier) else {
                continue;
            };
            style.push((target, ctx));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ElementTargetInfo, StyleTargetType,
        css::{parse_target, parse_target_type, pop_exit_media_selector},
    };
    #[test]
    fn test_parse_target() {
        let a = parse_target("div #div h1.div p").unwrap();
        let expected = vec![
            StyleTargetType::ElementType(String::from("div")),
            StyleTargetType::Id(String::from("div"), None),
            StyleTargetType::Class(String::from("div"), Some(String::from("h1"))),
            StyleTargetType::ElementType(String::from("p")),
        ];
        for (item, expected) in a.types.iter().zip(expected.iter()) {
            assert_eq!(item, expected)
        }
        // try matching them
        let mut info = vec![
            // an initial extra item that shouldnt affect anything, it should look at ancestors from recent to old
            ElementTargetInfo {
                type_name: "initial extra whatever",
                id: None,
                classes: vec![],
            },
            // all following elements replicate the expected structure of test target
            ElementTargetInfo {
                type_name: "div",
                id: None,
                classes: vec![],
            },
            ElementTargetInfo {
                type_name: "whatver",
                id: Some(String::from("div")),
                classes: vec![],
            },
            ElementTargetInfo {
                type_name: "h1",
                id: None,
                classes: vec![String::from("div")],
            },
            ElementTargetInfo {
                type_name: "p",
                id: None,
                classes: vec![],
            },
        ];
        assert!(a.matches(&info));
        // remove item and try again (should fail)
        info.pop();
        assert!(!a.matches(&info));

        // test single items

        let b = parse_target("#item").unwrap();
        let element = [ElementTargetInfo {
            type_name: "p",
            id: Some(String::from("item")),
            classes: vec![],
        }];
        assert!(b.matches(&element));

        // should fail
        let c = parse_target("div").unwrap();
        let element = [ElementTargetInfo {
            type_name: "p",
            id: Some(String::from("item")),
            classes: vec![],
        }];
        assert!(!c.matches(&element));
    }

    #[test]
    fn test_parse_target_type() {
        assert_eq!(
            parse_target_type("div#wa", None),
            Some(StyleTargetType::Id(
                "wa".to_string(),
                Some("div".to_string())
            ))
        );
        assert_eq!(
            parse_target_type("#wa", None),
            Some(StyleTargetType::Id("wa".to_string(), None))
        );
        assert_eq!(
            parse_target_type(".wa", None),
            Some(StyleTargetType::Class("wa".to_string(), None))
        );
        assert_eq!(
            parse_target_type("h1", None),
            Some(StyleTargetType::ElementType("h1".to_string()))
        );
    }

    #[test]
    fn test_pop_until_outside() {
        let mut chars: Vec<char> = "wahoo { h { rgr grg} wello {w aw a wa} }hello {wa}"
            .chars()
            .collect();
        chars.reverse();
        let (a, b) = pop_exit_media_selector(&mut chars);
        chars.reverse();
        // assert that the remaining characters will be untouched and expected
        assert_eq!(chars, "hello {wa}".chars().collect::<Vec<char>>());
        // assert that the media text and selector content is correct
        assert_eq!(&a, "wahoo ");
        assert_eq!(&b, " h { rgr grg} wello {w aw a wa} ");
    }
}

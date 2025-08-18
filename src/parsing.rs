use std::{collections::HashMap, sync::LazyLock};

use regex::{Captures, Regex};

use crate::{
    Webpage,
    css::parse_stylesheet,
    element::{DEFAULT_ELEMENT_TYPE, Element, get_element_type},
    utils::*,
};

enum ParseState {
    InElementType(String, HashMap<String, String>),
    WaitingForElement,
}

fn find_title(element: &Element) -> Option<&Element> {
    if element.ty.name == "title" {
        return Some(element);
    }
    for child in element.children.iter() {
        let title = find_title(child);
        if title.is_some() {
            return title;
        }
    }
    None
}
fn get_all_styles(element: &Element, buf: &mut String) {
    if element.ty.name == "style"
        && let Some(text) = &element.text
    {
        *buf += text
    }
    for child in element.children.iter() {
        get_all_styles(child, buf);
    }
}

static DECIMAL_ENCODING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"&#[\d]{1,4};").unwrap());

static HEX_ENCODING_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&#x[\d]{1,4};").unwrap());

fn parse_unicode(text: &str) -> String {
    let a = DECIMAL_ENCODING_RE
        .replace_all(text, |caps: &Captures| {
            let text: &str = &caps[0][2..caps[0].len() - 1];
            if let Ok(parsed) = text.parse::<u32>() {
                if let Some(char) = char::from_u32(parsed) {
                    return char.to_string();
                }
            }

            caps[0].to_string()
        })
        .to_string();
    HEX_ENCODING_RE
        .replace_all(&a, |caps: &Captures| {
            let text: &str = &caps[0][2..caps[0].len() - 1];
            if let Ok(parsed) = u32::from_str_radix(text, 16) {
                if let Some(char) = char::from_u32(parsed) {
                    return char.to_string();
                }
            }

            caps[0].to_string()
        })
        .to_string()
}

/// Replaces HTML special character encodings, like &amp; with their actual drawable character, in this case, &
///
/// Also replaces `&#nnnn;` where `nnnn` are digits, with the corresponding character with code of `nnnn`, and same for `&#xhhhh`, where `hhhh` are hexadecimal digits
///
/// Source: https://en.wikipedia.org/wiki/Character_encodings_in_HTML#Character_references
pub fn parse_special(text: &str) -> String {
    let new = text
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"");
    parse_unicode(&new)
}

pub fn parse_html(text: &str) -> Option<Webpage> {
    let mut buf: Vec<char> = text.trim().chars().collect();
    buf.reverse();
    let root = parse(&mut buf).pop();
    let mut title = None;
    let mut global_style = Vec::new();
    if let Some(root) = &root {
        title = find_title(root).map(|element| element.text.clone().unwrap());
        let mut all_styles = String::new();
        get_all_styles(root, &mut all_styles);
        parse_stylesheet(&all_styles, &mut global_style);
    }
    root.map(|root| Webpage {
        title,
        global_style,
        root: Some(root),
        ..Default::default()
    })
}

pub fn parse(buf: &mut Vec<char>) -> Vec<Element> {
    let mut elements = Vec::new();
    let mut state = ParseState::WaitingForElement;
    while let Some(char) = buf.pop() {
        match &mut state {
            ParseState::InElementType(name, attributes) => {
                if char == '>' {
                    if let ParseState::InElementType(name, attributes) = state {
                        let mut element = Element::new(
                            get_element_type(name.trim()).unwrap_or(&DEFAULT_ELEMENT_TYPE),
                        );
                        element.set_attributes(attributes);
                        if !element.ty.void_element && !element.ty.stops_parsing {
                            element.children = parse(buf);
                        } else if element.ty.stops_parsing {
                            let chars: Vec<char> = format!("</{name}>").chars().collect();
                            let text = pop_until_all(buf, &chars);
                            element.text = Some(text[..].iter().collect());
                        }
                        elements.push(element);
                        state = ParseState::WaitingForElement;
                    }
                    continue;
                } else if char == '/' {
                    buf.pop();
                    if let ParseState::InElementType(name, attributes) = state {
                        let mut element = Element::new(
                            get_element_type(name.trim()).unwrap_or(&DEFAULT_ELEMENT_TYPE),
                        );
                        element.set_attributes(attributes);
                        elements.push(element);
                        state = ParseState::WaitingForElement;
                    }
                    continue;
                } else if char.is_whitespace() {
                    let (key, end) = pop_until_any(buf, &['=', '/', '>']);
                    let Some(end) = end else {
                        continue;
                    };
                    if end != '=' {
                        buf.push(end);
                        continue;
                    }
                    let Some(quote_type) = buf.pop() else {
                        continue;
                    };
                    let value = pop_until(buf, &quote_type)
                        .iter()
                        .collect::<String>()
                        .trim()
                        .to_string();

                    let key = key.iter().collect::<String>().trim().to_string();
                    attributes.insert(key, value);

                    continue;
                } else {
                    name.push(char);
                }
            }
            ParseState::WaitingForElement => {
                if char == '<' {
                    if next_is(buf, &'/') {
                        pop_until(buf, &'>');
                        return elements;
                    }
                    if next_is(buf, &'!') {
                        buf.pop();
                        // if next characters are "--", that means we're in a comment
                        if buf.pop().is_some_and(|c| c == '-')
                            && buf.pop().is_some_and(|c| c == '-')
                        {
                            pop_until_all(buf, &['-', '-', '>']);
                        } else {
                            // otherwise, pop until ">", we're probably in a <!DOCTYPE html>
                            pop_until(buf, &'>');
                        }
                        continue;
                    }
                    state = ParseState::InElementType(String::new(), HashMap::new());

                    continue;
                }
                if let Some(Some(text)) = elements.last_mut().map(|f| &mut f.text) {
                    text.push(char);
                } else {
                    let mut element = Element::new(get_element_type("node").unwrap());
                    element.text = Some(String::from(char));
                    elements.push(element);
                }
            }
        }
    }
    elements
}

#[cfg(test)]
mod tests {
    use crate::parsing::parse_special;

    #[test]
    fn test_character_encoding() {
        assert_eq!(
            parse_special("nachos &amp; chips"),
            "nachos & chips".to_string()
        )
    }
}

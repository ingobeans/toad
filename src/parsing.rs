use std::collections::HashMap;

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

pub fn parse_html(text: &str) -> Option<Webpage> {
    let mut buf: Vec<char> = text.trim().chars().collect();
    buf.reverse();
    let root = parse(&mut buf).pop();
    let mut title = None;
    let mut global_style = HashMap::new();
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

use std::collections::HashMap;

use crate::{
    element::{get_element_type, Element, DEFAULT_ELEMENT_TYPE},
    Webpage,
};

enum ParseState {
    InElementType(String, HashMap<String, String>),
    WaitingForElement,
}

pub fn parse_html(text: &str) -> Option<Webpage> {
    let mut buf: Vec<char> = text.chars().collect();
    buf.reverse();
    let root = parse(&mut buf).pop();
    root.map(|root| Webpage {
        title: None,
        url: None,
        root: Some(root),
    })
}

fn pop_until<T: PartialEq>(a: &mut Vec<T>, b: &T) -> Vec<T> {
    let mut popped = Vec::new();
    while let Some(item) = a.pop() {
        if &item == b {
            return popped;
        }
        popped.push(item);
    }
    popped
}
fn pop_until_any<T: PartialEq>(a: &mut Vec<T>, b: &[T]) -> (Vec<T>, Option<T>) {
    let mut popped = Vec::new();
    while let Some(item) = a.pop() {
        if b.contains(&item) {
            return (popped, Some(item));
        }
        popped.push(item);
    }
    (popped, None)
}
fn pop_until_all<T: PartialEq>(a: &mut Vec<T>, b: &[T]) -> Vec<T> {
    let mut match_index = 0;
    let mut popped = Vec::new();
    while let Some(item) = a.pop() {
        if b[match_index] == item {
            match_index += 1;
            if match_index >= b.len() {
                return popped;
            }
            continue;
        }
        match_index = 0;
        popped.push(item);
    }
    popped
}

fn next_is<T: PartialEq>(a: &[T], b: &T) -> bool {
    let Some(item) = a.last() else {
        return false;
    };
    item == b
}

#[test]
fn wa() {
    let mut buf: Vec<char> = "i am teereere hello world".chars().collect();
    buf.reverse();
    let chars: Vec<char> = format!("hello").chars().collect();
    pop_until_all(&mut buf, &chars);
    println!("{buf:?}");
}

pub fn parse(buf: &mut Vec<char>) -> Vec<Element> {
    let mut elements = Vec::new();
    let mut state = ParseState::WaitingForElement;
    while let Some(char) = buf.pop() {
        match &mut state {
            ParseState::InElementType(name, attributes) => {
                if char == '>' {
                    if let ParseState::InElementType(name, attributes) = state {
                        let mut element =
                            Element::new(get_element_type(&name).unwrap_or(&DEFAULT_ELEMENT_TYPE));
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
                        let mut element =
                            Element::new(get_element_type(&name).unwrap_or(&DEFAULT_ELEMENT_TYPE));
                        element.set_attributes(attributes);
                        elements.push(element);
                        state = ParseState::WaitingForElement;
                    }
                    continue;
                } else if char == ' ' {
                    let (key, end) = pop_until_any(buf, &['=', '/', '>']);
                    let Some(end) = end else {
                        continue;
                    };
                    if end != '=' {
                        buf.push(end);
                        continue;
                    }
                    buf.pop();
                    let value = pop_until(buf, &'"').iter().collect();

                    let key = key.iter().collect();
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
                        pop_until(buf, &'>');
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

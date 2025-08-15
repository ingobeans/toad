use crate::{
    Webpage,
    element::{DEFAULT_ELEMENT_TYPE, Element, get_element_type},
};

enum ParseState {
    InElementType(String),
    WaitingForElement,
}

pub fn parse_html(text: &str) -> Option<Webpage> {
    let mut buf: Vec<char> = text.chars().collect();
    buf.reverse();
    let root = parse(&mut buf).pop();
    root.map(|root| Webpage {
        title: None,
        url: None,
        body: Some(root),
    })
}

fn pop_until<T: PartialEq>(a: &mut Vec<T>, b: &T) {
    while let Some(popped) = a.pop() {
        if &popped == b {
            return;
        }
    }
}

fn next_is<T: PartialEq>(a: &mut Vec<T>, b: &T) -> bool {
    let Some(popped) = a.last() else {
        return false;
    };
    popped == b
}

pub fn parse(buf: &mut Vec<char>) -> Vec<Element> {
    let mut elements = Vec::new();
    let mut state = ParseState::WaitingForElement;
    while let Some(char) = buf.pop() {
        match &mut state {
            ParseState::InElementType(name) => {
                if char == '>' {
                    let mut element = Element {
                        ty: get_element_type(&name).unwrap_or(&DEFAULT_ELEMENT_TYPE),
                        children: Vec::new(),
                        text: None,
                    };
                    if !element.ty.stops_parsing {
                        element.children = parse(buf);
                    }
                    elements.push(element);
                    state = ParseState::WaitingForElement;

                    continue;
                }
                name.push(char);
            }
            ParseState::WaitingForElement => {
                if char == '<' {
                    if next_is(buf, &'/') {
                        pop_until(buf, &'>');
                        return elements;
                    }
                    state = ParseState::InElementType(String::new());

                    continue;
                }
                if let Some(Some(text)) = elements.last_mut().map(|f| &mut f.text) {
                    text.push(char);
                } else {
                    elements.push(Element {
                        ty: get_element_type("node").unwrap(),
                        children: Vec::new(),
                        text: Some(String::from(char)),
                    });
                }
            }
            _ => {}
        }
    }
    elements
}

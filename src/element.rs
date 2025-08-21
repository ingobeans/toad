use std::{
    collections::HashMap,
    fmt::Debug,
    io::{self, Write},
};

use crate::{
    ActualMeasurement, DEFAULT_DRAW_CTX, Display, DrawCall, ElementDrawContext, ElementTargetInfo,
    GlobalDrawContext, InteractableElement, InteractableType, Measurement, NonInheritedField::*,
    TextPrefix, consts::*, css, parsing::parse_special,
};
use crossterm::{queue, style};

const RED: style::Color = style::Color::Red;

#[derive(Clone, Copy, PartialEq)]
pub struct ElementType {
    pub name: &'static str,
    pub stops_parsing: bool,
    /// Element that has no closing tag, such as <img>
    pub void_element: bool,
    pub draw_ctx: ElementDrawContext,
}
pub static DEFAULT_ELEMENT_TYPE: ElementType = ElementType {
    name: "unknown",
    stops_parsing: false,
    void_element: false,
    draw_ctx: DEFAULT_DRAW_CTX,
};
pub static NODE: ElementType = ElementType {
    name: "node",
    draw_ctx: ElementDrawContext {
        width: Specified(Measurement::FitContentWidth),
        height: Specified(Measurement::FitContentHeight),
        background_color: Inherit,
        ..DEFAULT_DRAW_CTX
    },
    ..DEFAULT_ELEMENT_TYPE
};
static P: ElementType = ElementType {
    name: "p",
    draw_ctx: ElementDrawContext {
        display: Specified(Display::Block),
        width: Specified(Measurement::FitContentWidth),
        height: Specified(Measurement::FitContentHeight),
        ..DEFAULT_DRAW_CTX
    },
    ..DEFAULT_ELEMENT_TYPE
};
static B: ElementType = ElementType {
    name: "b",
    draw_ctx: ElementDrawContext {
        bold: true,
        width: Specified(Measurement::FitContentWidth),
        height: Specified(Measurement::FitContentHeight),
        ..DEFAULT_DRAW_CTX
    },
    ..SPAN
};
static H1: ElementType = ElementType {
    name: "h1",
    draw_ctx: ElementDrawContext {
        bold: true,
        foreground_color: Some(RED),
        display: Specified(Display::Block),
        width: Specified(Measurement::FitContentWidth),
        height: Specified(Measurement::FitContentHeight),
        ..DEFAULT_DRAW_CTX
    },
    ..DEFAULT_ELEMENT_TYPE
};
static BR: ElementType = ElementType {
    name: "br",
    void_element: true,
    draw_ctx: ElementDrawContext {
        display: Specified(Display::Block),
        height: Specified(Measurement::Pixels(LH)),
        ..DEFAULT_DRAW_CTX
    },
    ..DEFAULT_ELEMENT_TYPE
};
static DIV: ElementType = ElementType {
    name: "div",
    draw_ctx: ElementDrawContext {
        display: Specified(Display::Block),
        height: Specified(Measurement::FitContentHeight),
        width: Specified(Measurement::FitContentWidth),
        ..DEFAULT_DRAW_CTX
    },
    ..DEFAULT_ELEMENT_TYPE
};
static SPAN: ElementType = ElementType {
    name: "span",
    draw_ctx: ElementDrawContext {
        width: Specified(Measurement::FitContentWidth),
        height: Specified(Measurement::FitContentHeight),
        ..DEFAULT_DRAW_CTX
    },
    ..DEFAULT_ELEMENT_TYPE
};
static BODY: ElementType = ElementType {
    name: "body",
    draw_ctx: ElementDrawContext {
        width: Specified(Measurement::PercentWidth(1.0)),
        height: Specified(Measurement::PercentHeight(1.0)),
        background_color: Specified(DEFAULT_BACKGROUND_COLOR),
        display: Specified(Display::Block),
        ..DEFAULT_DRAW_CTX
    },
    ..DEFAULT_ELEMENT_TYPE
};
static PRE: ElementType = ElementType {
    name: "pre",
    draw_ctx: ElementDrawContext {
        respect_whitespace: true,
        width: Specified(Measurement::FitContentWidth),
        height: Specified(Measurement::FitContentHeight),
        display: Specified(Display::Block),
        ..DEFAULT_DRAW_CTX
    },
    ..DEFAULT_ELEMENT_TYPE
};
static EM_TAG: ElementType = ElementType {
    name: "em",
    draw_ctx: ElementDrawContext {
        italics: true,
        width: Specified(Measurement::FitContentWidth),
        height: Specified(Measurement::FitContentHeight),
        ..DEFAULT_DRAW_CTX
    },
    ..SPAN
};
pub static ELEMENT_TYPES: &[ElementType] = &[
    BODY,
    P,
    BR,
    DIV,
    SPAN,
    B,
    EM_TAG,
    PRE,
    ElementType {
        name: "i",
        ..EM_TAG
    },
    ElementType {
        name: "code",
        draw_ctx: ElementDrawContext {
            respect_whitespace: true,
            width: Specified(Measurement::FitContentWidth),
            height: Specified(Measurement::FitContentHeight),
            display: Specified(Display::Inline),
            ..DEFAULT_DRAW_CTX
        },
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "strong",
        ..B
    },
    ElementType {
        name: "section",
        ..DIV
    },
    ElementType {
        name: "cite",
        ..EM_TAG
    },
    ElementType {
        name: "blockquote",
        ..PRE
    },
    ElementType { name: "dl", ..P },
    ElementType { name: "dt", ..P },
    ElementType { name: "dd", ..P },
    ElementType {
        name: "font",
        ..SPAN
    },
    ElementType {
        name: "footer",
        ..P
    },
    ElementType {
        name: "main",
        ..BODY
    },
    ElementType {
        name: "article",
        ..DIV
    },
    ElementType {
        name: "label",
        ..SPAN
    },
    ElementType {
        name: "picture",
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "source",
        void_element: true,
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "button",
        ..SPAN
    },
    ElementType {
        name: "figcaption",
        ..P
    },
    ElementType {
        name: "figure",
        ..DIV
    },
    ElementType {
        name: "form",
        ..DIV
    },
    ElementType {
        name: "sup",
        ..SPAN
    },
    ElementType {
        name: "ol",
        draw_ctx: ElementDrawContext {
            display: Specified(Display::Block),
            width: Specified(Measurement::FitContentWidth),
            height: Specified(Measurement::FitContentHeight),
            text_prefix: Some(TextPrefix::Number),
            ..DEFAULT_DRAW_CTX
        },
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "ul",
        draw_ctx: ElementDrawContext {
            display: Specified(Display::Block),
            width: Specified(Measurement::FitContentWidth),
            height: Specified(Measurement::FitContentHeight),
            text_prefix: Some(TextPrefix::Dot),
            ..DEFAULT_DRAW_CTX
        },
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType { name: "li", ..P },
    ElementType {
        name: "html",
        ..BODY
    },
    ElementType {
        name: "meta",
        void_element: true,
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "img",
        void_element: true,
        draw_ctx: ElementDrawContext {
            width: Specified(Measurement::Pixels(25 * EM)),
            height: Specified(Measurement::Pixels(10 * LH)),
            display: Specified(Display::Block),
            ..DEFAULT_DRAW_CTX
        },
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "input",
        void_element: true,
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "hr",
        void_element: true,
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType { name: "nav", ..DIV },
    ElementType {
        name: "bdi",
        ..SPAN
    },
    ElementType {
        name: "head",
        draw_ctx: ElementDrawContext {
            display: Specified(Display::None),
            ..DEFAULT_DRAW_CTX
        },
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "link",
        void_element: true,
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "title",
        stops_parsing: true,
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "header",
        ..P
    },
    ElementType { name: "a", ..SPAN },
    ElementType {
        name: "style",
        stops_parsing: true,
        ..DEFAULT_ELEMENT_TYPE
    },
    ElementType {
        name: "script",
        stops_parsing: true,
        ..DEFAULT_ELEMENT_TYPE
    },
    H1,
    ElementType { name: "h2", ..H1 },
    ElementType { name: "h3", ..H1 },
    ElementType { name: "h4", ..H1 },
    ElementType { name: "h5", ..H1 },
    ElementType { name: "h6", ..H1 },
];
pub fn fit_text_in_width(
    text: &str,
    parent_width: ActualMeasurement,
    starting_x: u16,
) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut x = starting_x / EM;
    let parent_width = parent_width.get_pixels();
    for char in text.chars() {
        if char == '\n' {
            x = 0;
            parts.push(String::new());
            continue;
        } else {
            x += 1;
        }
        parts.last_mut().unwrap().push(char);
        if let Some(parent_width) = parent_width
            && x >= parent_width / EM
        {
            x = 0;
            parts.push(String::new());
        }
    }
    parts
}
pub fn get_element_type(name: &str) -> Option<&'static ElementType> {
    if !ELEMENT_TYPES.iter().any(|f| f.name == name) {
        //panic!("WA::: {name:?}")
    }
    ELEMENT_TYPES.iter().find(|f| f.name == name)
}
/// Removes repeated whitespace and newlines
fn disrespect_whitespace(text: &str, allow_leading: bool) -> String {
    let text = text.replace("\n", "").replace("\r", "");
    let mut new = String::new();
    let mut last_was_whitespace = !allow_leading;
    for char in text.chars() {
        if char.is_whitespace() {
            if last_was_whitespace {
                continue;
            }
            last_was_whitespace = true;
        } else {
            last_was_whitespace = false;
        }
        new.push(char);
    }
    new
}
fn is_whitespace(text: &str) -> bool {
    text.chars().all(|c| c.is_whitespace())
}
pub fn apply_draw_ctx<T: Write>(
    draw_ctx: ElementDrawContext,
    old_ctx: &mut ElementDrawContext,
    stdout: &mut T,
) -> io::Result<()> {
    if *old_ctx == draw_ctx {
        return Ok(());
    }
    let needs_clearing = (!draw_ctx.bold && old_ctx.bold) || (!draw_ctx.italics && old_ctx.italics);
    let foreground_color = draw_ctx.foreground_color.unwrap_or(style::Color::Black);
    let background_color = draw_ctx
        .background_color
        .unwrap_or(DEFAULT_BACKGROUND_COLOR);

    if needs_clearing {
        queue!(stdout, style::ResetColor)?;
        old_ctx.bold = false;
        old_ctx.italics = false;
        old_ctx.foreground_color = None;
    }
    let mut attributes = style::Attributes::none();

    if draw_ctx.bold {
        attributes.set(style::Attribute::Bold);
    }
    if draw_ctx.italics {
        attributes.set(style::Attribute::Italic);
    }

    queue!(
        stdout,
        style::SetStyle(style::ContentStyle {
            foreground_color: Some(foreground_color),
            background_color: Some(background_color),
            attributes,
            ..Default::default()
        })
    )?;

    old_ctx.bold = draw_ctx.bold;
    old_ctx.italics = draw_ctx.italics;
    old_ctx.foreground_color = draw_ctx.foreground_color;
    old_ctx.background_color = draw_ctx.background_color;
    Ok(())
}

fn actualize(
    a: Measurement,
    draw_data: &DrawData,
    unknown_sized_elements: &mut Vec<Option<ActualMeasurement>>,
    content_size_known: bool,
) -> ActualMeasurement {
    match a {
        Measurement::Pixels(pixels) => ActualMeasurement::Pixels(pixels),
        Measurement::FitContentHeight if content_size_known => {
            ActualMeasurement::Pixels(draw_data.content_height)
        }
        Measurement::FitContentWidth if content_size_known => {
            ActualMeasurement::Pixels(draw_data.content_width)
        }
        Measurement::PercentHeight(percent) => match draw_data.parent_height {
            ActualMeasurement::Pixels(pixels) => {
                ActualMeasurement::Pixels((pixels as f32 * percent) as u16)
            }
            ActualMeasurement::PercentOfUnknown(index, p) => {
                ActualMeasurement::PercentOfUnknown(index, percent * p)
            }
            ActualMeasurement::Waiting(index) => {
                ActualMeasurement::PercentOfUnknown(index, percent)
            }
        },
        Measurement::PercentWidth(percent) => match draw_data.parent_width {
            ActualMeasurement::Pixels(pixels) => {
                ActualMeasurement::Pixels((pixels as f32 * percent) as u16)
            }
            ActualMeasurement::PercentOfUnknown(index, p) => {
                ActualMeasurement::PercentOfUnknown(index, percent * p)
            }
            ActualMeasurement::Waiting(index) => {
                ActualMeasurement::PercentOfUnknown(index, percent)
            }
        },
        _ => {
            let index = unknown_sized_elements.len();
            unknown_sized_elements.push(None);
            ActualMeasurement::Waiting(index)
        }
    }
}
#[derive(Default, Clone)]
pub struct DrawData {
    pub draw_calls: Vec<DrawCall>,
    pub content_width: u16,
    pub content_height: u16,
    pub parent_width: ActualMeasurement,
    pub parent_height: ActualMeasurement,
    pub x: u16,
    pub y: u16,
    pub parent_interactable: Option<InteractableElement>,
    pub ancestors_target_info: Vec<ElementTargetInfo>,
    pub last_item_height: u16,
    /// Condition set to true if the previous element drawn with this context was both `display: inline`,
    /// and had a non-zero width. Used to tell whether a leading whitespace should be allowed for text.
    pub last_was_inline_and_sized: bool,
}
pub struct Element {
    pub ty: &'static ElementType,
    pub children: Vec<Element>,
    attributes: HashMap<String, String>,
    pub style: ElementDrawContext,
    pub text: Option<String>,
    pub classes: Vec<String>,
}
impl Debug for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.print_recursive(0))
    }
}
impl Element {
    pub fn new(ty: &'static ElementType) -> Self {
        Self {
            ty,
            children: Vec::new(),
            attributes: HashMap::new(),
            classes: Vec::new(),
            style: DEFAULT_DRAW_CTX,
            text: None,
        }
    }
    pub fn get_attribute(&self, k: &str) -> Option<&String> {
        self.attributes.get(k)
    }
    pub fn set_attributes(&mut self, attributes: HashMap<String, String>) {
        if let Some(style) = attributes.get("style") {
            css::parse_ruleset(style, &mut self.style);
        }
        if let Some(class) = attributes.get("class") {
            self.classes = class.split(' ').map(|f| f.to_string()).collect();
        }
        self.attributes = attributes;
    }
    pub fn print_recursive(&self, index: usize) -> String {
        let children_text = match &self.text {
            Some(text) => text.clone(),
            None => {
                let mut children_text = String::new();
                for child in &self.children {
                    children_text += "\n";
                    children_text += &child.print_recursive(index + 1);
                }
                children_text
            }
        };
        let padding = "\t".repeat(index);
        let mut attributes = String::new();
        for (k, v) in &self.attributes {
            attributes += &format!(" {k}=\"{v}\"");
        }
        let mut maybe_newline = format!("\n{padding}");
        if !children_text.contains("\n") {
            maybe_newline = String::new();
        }
        format!(
            "\n{padding}<{}{attributes}>{}{maybe_newline}</{}>",
            self.ty.name, children_text, self.ty.name
        )
    }
    pub fn get_active_style(
        &self,
        global_ctx: &GlobalDrawContext,
        parent_draw_context: ElementDrawContext,
        ancestor_target_info: &[ElementTargetInfo],
    ) -> ElementDrawContext {
        // construct this elements style by overlaying:
        //  - parent style
        //  - the base element's style
        //  - matching global styles
        //  - inline css

        let mut style = self.ty.draw_ctx;
        // merge_inherit will only fill inherited, unset fields of style
        style.merge_inherit(&parent_draw_context);

        for (k, v) in global_ctx.global_style.iter() {
            if k.matches(ancestor_target_info) {
                style.merge_all(v);
            }
        }
        style.merge_all(&self.style);

        // if this element is a <font> (https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/font)
        // make its "color" attribute overwrite the style's color.
        // this is an old deprecated tag from html4, still used in some places though
        if self.ty.name == "font"
            && let Some(color) = self.get_attribute("color")
        {
            style.foreground_color = css::parse_color(color).or(style.foreground_color);
        }

        // if this element is an <img>,
        // allow width and height attributes to affect style's width and height
        if self.ty.name == "img" {
            if let Some(Ok(width)) = self.get_attribute("width").map(|f| f.parse::<u16>()) {
                style.width = Specified(Measurement::Pixels(width));
            }
            if let Some(Ok(height)) = self.get_attribute("height").map(|f| f.parse::<u16>()) {
                // i legitimately do not know why this needs to be divided by two
                style.height = Specified(Measurement::Pixels(height / 2));
            }
        }

        // check all NonInheritedFields in case they are set to inherit, if so, inherit from parent_draw_context

        style
            .background_color
            .inherit_from(parent_draw_context.background_color);
        style.height.inherit_from(parent_draw_context.height);
        style.display.inherit_from(parent_draw_context.display);
        style
    }
    pub fn draw(
        &self,
        parent_draw_ctx: ElementDrawContext,
        global_ctx: &mut GlobalDrawContext,
        draw_data: &mut DrawData,
    ) -> io::Result<()> {
        let mut draw_data_ancestor_info = draw_data.ancestors_target_info.clone();
        draw_data_ancestor_info.push(ElementTargetInfo {
            type_name: self.ty.name,
            id: self.get_attribute("id").cloned(),
            classes: self.classes.clone(),
        });
        // construct this element's active style
        let style = self.get_active_style(global_ctx, parent_draw_ctx, &draw_data_ancestor_info);

        if self.ty.stops_parsing || matches!(style.display, Specified(Display::None)) {
            return Ok(());
        }

        let is_display_block = matches!(style.display, Specified(Display::Block));

        if is_display_block && draw_data.x != 0 {
            draw_data.y += draw_data.last_item_height.max(LH);
            draw_data.x = 0;
        }
        let mut self_interactable = draw_data.parent_interactable.clone();

        if self.ty.name == "node" {
            if let Some(text) = &self.text
                && (!is_whitespace(text) || style.respect_whitespace)
            {
                let mut text = if style.respect_whitespace {
                    text.clone()
                } else {
                    disrespect_whitespace(text, draw_data.last_was_inline_and_sized)
                };
                text = parse_special(&text);
                let mut lines = fit_text_in_width(&text, draw_data.parent_width, draw_data.x)
                    .into_iter()
                    .peekable();
                let mut any_text = false;

                while let Some(line) = lines.next() {
                    let len = line.len() as u16;
                    if len != 0 {
                        any_text = true;
                    }
                    draw_data.draw_calls.push(DrawCall::Text(
                        draw_data.x,
                        draw_data.y,
                        line,
                        style,
                        draw_data.parent_width,
                        draw_data.parent_interactable.clone(),
                    ));
                    draw_data.x += len * EM;
                    draw_data.content_width = draw_data.content_width.max(draw_data.x);
                    if lines.peek().is_some() {
                        draw_data.x = 0;
                        draw_data.y += LH;
                    }
                }
                draw_data.content_height = draw_data.content_height.max(draw_data.y + LH);

                draw_data.last_was_inline_and_sized = !is_display_block && any_text;
            }
            return Ok(());
        } else if self.ty.name == "a"
            && let Some(link) = self.get_attribute("href")
        {
            // register link as interactable element
            self_interactable = Some(InteractableElement {
                ty: InteractableType::Link(link.clone()),
                index: global_ctx.interactable_index,
            });
            global_ctx.interactable_index += 1;
        }

        // actualize width and height
        let mut actual_width = actualize(
            style.width.unwrap_or(Measurement::Pixels(0)),
            draw_data,
            &mut global_ctx.unknown_sized_elements,
            false,
        );
        let mut actual_height = actualize(
            style.height.unwrap_or(Measurement::Pixels(0)),
            draw_data,
            &mut global_ctx.unknown_sized_elements,
            false,
        );

        if self.ty.name == "img" {
            // make sure that there **never** exists any unfulfilled Waiting promises.
            // we have to do this here for images, since they have an early return
            if let ActualMeasurement::Waiting(wi) = actual_width {
                actual_width = ActualMeasurement::Pixels(0);
                global_ctx.unknown_sized_elements[wi] = Some(actual_width);
            }
            if let ActualMeasurement::Waiting(hi) = actual_height {
                actual_height = ActualMeasurement::Pixels(0);
                global_ctx.unknown_sized_elements[hi] = Some(actual_height);
            }
            if let Some(source) = self.get_attribute("src")
                && actual_width.get_pixels_lossy() > 0
                && actual_height.get_pixels_lossy() > 0
            {
                draw_data.draw_calls.push(DrawCall::Image(
                    draw_data.x,
                    draw_data.y,
                    actual_width,
                    actual_height,
                    source.clone(),
                ));
                draw_data.content_width =
                    draw_data.content_width.max(actual_width.get_pixels_lossy());
                draw_data.content_height = draw_data
                    .content_height
                    .max(actual_height.get_pixels_lossy());
            }

            draw_data.last_was_inline_and_sized = false;
            draw_data.x += actual_width.get_pixels_lossy();
            if is_display_block
                && let Some(h) = actual_height.get_pixels()
                && h > 0
            {
                draw_data.y += h;
                draw_data.x = 0;
            }
            return Ok(());
        }

        draw_data.content_width = draw_data.content_width.max(actual_width.get_pixels_lossy());
        draw_data.content_height = draw_data
            .content_height
            .max(actual_height.get_pixels_lossy());

        let draw_data_parent_width = if let Some(pixels) = draw_data.parent_width.get_pixels()
            && pixels != 0
            && actual_width.get_pixels().is_none_or(|p| p > pixels)
        {
            ActualMeasurement::Pixels(pixels)
        } else {
            actual_width
        };
        let mut child_data = DrawData {
            parent_width: draw_data_parent_width,
            parent_height: actual_height,
            parent_interactable: self_interactable,
            ancestors_target_info: draw_data_ancestor_info,
            last_was_inline_and_sized: draw_data.last_was_inline_and_sized,
            ..Default::default()
        };
        for child in self.children.iter() {
            child.draw(style, global_ctx, &mut child_data)?;
            draw_data.content_width = draw_data
                .content_width
                .max(draw_data.x + child_data.content_width);
            draw_data.content_height = draw_data
                .content_height
                .max(draw_data.y.saturating_add(child_data.content_height));
        }
        for draw_call in child_data.draw_calls.iter_mut() {
            match draw_call {
                DrawCall::Rect(x, y, _, _, _) => {
                    *x += draw_data.x;
                    *y += draw_data.y;
                }
                DrawCall::Image(x, y, _, _, _) => {
                    *x += draw_data.x;
                    *y += draw_data.y;
                }
                DrawCall::Text(x, y, _, _, _, _) => {
                    *x += draw_data.x;
                    *y += draw_data.y;
                }
            }
        }

        // reactualize width and height with content size known
        if let ActualMeasurement::Waiting(index) = actual_width {
            actual_width = actualize(
                style.width.unwrap_or(Measurement::Pixels(0)),
                &child_data,
                &mut global_ctx.unknown_sized_elements,
                true,
            );
            global_ctx.unknown_sized_elements[index] = Some(actual_width);
        }
        if let ActualMeasurement::Waiting(index) = actual_height {
            actual_height = actualize(
                style.height.unwrap_or(Measurement::Pixels(0)),
                &child_data,
                &mut global_ctx.unknown_sized_elements,
                true,
            );
            global_ctx.unknown_sized_elements[index] = Some(actual_height);
        }

        if is_display_block && let Specified(color) = style.background_color {
            draw_data.draw_calls.push(DrawCall::Rect(
                draw_data.x,
                draw_data.y,
                actual_width,
                actual_height,
                color,
            ));
        }

        let width = actual_width.get_pixels_lossy();
        draw_data.content_width = draw_data.content_width.max(width);
        let height = actual_height.get_pixels_lossy();
        draw_data.content_height = draw_data.content_height.max(height);
        draw_data.x += width;
        if is_display_block {
            draw_data.last_item_height = 0;
            draw_data.y = draw_data.y.saturating_add(height);
            draw_data.x = 0;
        } else {
            draw_data.last_item_height = height;
        }
        draw_data.last_was_inline_and_sized = !is_display_block && width > 0;

        draw_data.draw_calls.append(&mut child_data.draw_calls);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::element::disrespect_whitespace;

    #[test]
    fn test_disrespect_whitespace() {
        let a = "helo        there\nmy\nfriend";
        assert_eq!(
            disrespect_whitespace(a, false),
            String::from("helo theremyfriend")
        );
        let b = "\t\t   hi";
        assert_eq!(disrespect_whitespace(b, false), String::from("hi"));
        assert_eq!(disrespect_whitespace(b, true), String::from("\thi"));
    }
}

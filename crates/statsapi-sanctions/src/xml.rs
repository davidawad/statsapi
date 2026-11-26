//! A small streaming walker over quick-xml events with an element path.

use crate::ClientError;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use std::io::BufRead;

/// An opening tag and its attributes.
pub struct Start<'a>(&'a BytesStart<'a>);

impl Start<'_> {
    /// The unescaped value of an attribute, if present.
    pub fn attribute(&self, name: &str) -> Option<String> {
        let attribute = self.0.try_get_attribute(name).ok()??;
        Some(String::from_utf8_lossy(&attribute.value).into_owned())
    }
}

/// One step of the walk. `stack` holds the open elements, innermost last; for
/// `Start` and `End` it includes the element itself.
pub enum Node<'a> {
    Start(Start<'a>),
    Text(&'a str),
    End,
}

/// True when the innermost open elements are exactly `tail`, in order.
pub fn ends_with(stack: &[Vec<u8>], tail: &[&str]) -> bool {
    stack.len() >= tail.len()
        && stack[stack.len() - tail.len()..]
            .iter()
            .zip(tail)
            .all(|(open, wanted)| open.as_slice() == wanted.as_bytes())
}

/// Streams `reader`, calling `visit` for each start tag, non-blank text, and
/// end tag.
pub fn walk<R: BufRead>(
    reader: R,
    mut visit: impl FnMut(Node<'_>, &[Vec<u8>]),
) -> Result<(), ClientError> {
    let mut reader = Reader::from_reader(reader);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut stack: Vec<Vec<u8>> = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(start) => {
                stack.push(start.name().as_ref().to_vec());
                visit(Node::Start(Start(&start)), &stack);
            }
            Event::Empty(start) => {
                stack.push(start.name().as_ref().to_vec());
                visit(Node::Start(Start(&start)), &stack);
                visit(Node::End, &stack);
                stack.pop();
            }
            Event::Text(text) => {
                if let Ok(text) = text.decode() {
                    visit(Node::Text(&text), &stack);
                }
            }
            Event::End(_) => {
                visit(Node::End, &stack);
                stack.pop();
            }
            Event::Eof => return Ok(()),
            _ => {}
        }
        buffer.clear();
    }
}

//! Builders for [`Layer`] and [`Field`].

use crate::{Field, Layer};

impl Field {
    pub(crate) fn new(label: &str, value: impl Into<String>, start: usize, length: usize) -> Self {
        Field { label: label.to_string(), value: value.into(), explain: None, start, length, children: Vec::new() }
    }

    /// A field that is not tied to bytes.
    pub(crate) fn note(label: &str, value: impl Into<String>) -> Self {
        Field::new(label, value, 0, 0)
    }

    pub(crate) fn explain(mut self, text: impl Into<String>) -> Self {
        self.explain = Some(text.into());
        self
    }

    pub(crate) fn child(mut self, child: Field) -> Self {
        self.children.push(child);
        self
    }

    pub(crate) fn children(mut self, children: Vec<Field>) -> Self {
        self.children.extend(children);
        self
    }

    /// Keeps the byte range inside the frame, here and in every child.
    fn clamp(&mut self, frame_len: usize) {
        clamp_range(&mut self.start, &mut self.length, frame_len);
        for child in &mut self.children {
            child.clamp(frame_len);
        }
    }
}

impl Layer {
    /// A layer covering the bytes from `start` to `end`.
    pub(crate) fn new(
        id: &str,
        title: &str,
        summary: impl Into<String>,
        explain: &str,
        start: usize,
        end: usize,
    ) -> Self {
        Layer {
            id: id.to_string(),
            title: title.to_string(),
            summary: summary.into(),
            explain: explain.to_string(),
            start,
            length: end.saturating_sub(start),
            fields: Vec::new(),
        }
    }

    pub(crate) fn push(&mut self, field: Field) {
        self.fields.push(field);
    }

    /// Keeps every byte range inside the frame, so a hex view can trust them.
    pub(crate) fn clamp(&mut self, frame_len: usize) {
        clamp_range(&mut self.start, &mut self.length, frame_len);
        for field in &mut self.fields {
            field.clamp(frame_len);
        }
    }
}

fn clamp_range(start: &mut usize, length: &mut usize, frame_len: usize) {
    *start = (*start).min(frame_len);
    *length = (*length).min(frame_len - *start);
}

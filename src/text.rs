use tower_lsp::lsp_types::{Position, Range, TextDocumentContentChangeEvent};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn contains(&self, offset: usize) -> bool {
        self.start <= offset && offset <= self.end
    }

    pub fn cover(self, other: Span) -> Span {
        Span::new(self.start.min(other.start), self.end.max(other.end))
    }
}

pub fn span_to_range(text: &str, span: Span) -> Range {
    Range {
        start: offset_to_position(text, span.start),
        end: offset_to_position(text, span.end),
    }
}

pub fn offset_to_position(text: &str, offset: usize) -> Position {
    let offset = offset.min(text.len());
    let mut line = 0u32;
    let mut line_start = 0usize;

    for (idx, ch) in text.char_indices() {
        if idx >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = idx + ch.len_utf8();
        }
    }

    let character = text[line_start..offset]
        .chars()
        .map(char::len_utf16)
        .sum::<usize>() as u32;

    Position { line, character }
}

pub fn position_to_offset(text: &str, position: Position) -> usize {
    let mut line = 0u32;
    let mut line_start = 0usize;

    for (idx, ch) in text.char_indices() {
        if line == position.line {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = idx + ch.len_utf8();
        }
    }

    if line < position.line {
        return text.len();
    }

    let mut utf16 = 0u32;
    let mut offset = line_start;
    for ch in text[line_start..].chars() {
        if ch == '\n' || utf16 >= position.character {
            break;
        }

        let width = ch.len_utf16() as u32;
        if utf16 + width > position.character {
            break;
        }

        utf16 += width;
        offset += ch.len_utf8();
    }

    offset
}

pub fn apply_content_changes(
    mut text: String,
    changes: Vec<TextDocumentContentChangeEvent>,
) -> String {
    for change in changes {
        match change.range {
            Some(range) => {
                let start = position_to_offset(&text, range.start);
                let end = position_to_offset(&text, range.end);
                if start <= end && end <= text.len() {
                    text.replace_range(start..end, &change.text);
                }
            }
            None => text = change.text,
        }
    }

    text
}

pub fn range_overlaps(left: &Range, right: &Range) -> bool {
    !(left.end < right.start || right.end < left.start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_position_conversion_handles_utf16() {
        let text = "alpha\nmeow 😸\nomega";
        let position = Position {
            line: 1,
            character: 5,
        };

        let offset = position_to_offset(text, position);
        assert_eq!(&text[offset..], "😸\nomega");
        assert_eq!(offset_to_position(text, offset), position);
    }

    #[test]
    fn incremental_changes_apply_in_order() {
        let text = "abc".to_string();
        let changed = apply_content_changes(
            text,
            vec![TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: 0,
                        character: 1,
                    },
                    end: Position {
                        line: 0,
                        character: 2,
                    },
                }),
                range_length: None,
                text: "zz".to_string(),
            }],
        );

        assert_eq!(changed, "azzc");
    }
}

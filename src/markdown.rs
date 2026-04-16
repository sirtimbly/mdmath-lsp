use crate::text::Span;

#[derive(Clone, Debug)]
pub struct ExtractedStatement {
    pub text: String,
    pub source_span: Span,
    pub display_span: Span,
    pub insert_span: Span,
}

pub fn extract_statements(text: &str) -> Vec<ExtractedStatement> {
    let mut statements = Vec::new();
    let mut in_math = false;
    let mut offset = 0usize;

    while offset <= text.len() {
        let line_end = text[offset..]
            .find('\n')
            .map(|idx| offset + idx)
            .unwrap_or(text.len());
        let raw_line = &text[offset..line_end];
        let line = raw_line.trim_end_matches('\r');

        if in_math {
            if let Some((statement, next_offset)) = collect_list_statement(text, line, offset) {
                statements.push(statement);
                offset = next_offset;
                continue;
            }
            push_math_statement(&mut statements, line, offset);
        } else if let Some(rest) = line.strip_prefix("math:") {
            in_math = true;
            push_marker_statement(&mut statements, line, rest, offset);
        }

        if line_end == text.len() {
            break;
        }
        offset = line_end + 1;
    }

    statements
}

fn push_marker_statement(
    statements: &mut Vec<ExtractedStatement>,
    line: &str,
    rest: &str,
    line_offset: usize,
) {
    let trimmed = rest.trim_start();
    if trimmed.is_empty() {
        return;
    }

    let padding = rest.len() - trimmed.len();
    let start = line_offset + "math:".len() + padding;
    let end = line_offset + line.len();

    statements.push(ExtractedStatement {
        text: trimmed.to_string(),
        source_span: Span::new(start, end),
        display_span: Span::new(line_offset, end),
        insert_span: Span::new(end, end),
    });
}

fn push_math_statement(statements: &mut Vec<ExtractedStatement>, line: &str, line_offset: usize) {
    if line.trim().is_empty() {
        return;
    }

    let start_padding = line.len() - line.trim_start().len();
    let start = line_offset + start_padding;
    let end = line_offset + line.len();

    statements.push(ExtractedStatement {
        text: line.trim_start().to_string(),
        source_span: Span::new(start, end),
        display_span: Span::new(start, end),
        insert_span: Span::new(end, end),
    });
}

fn collect_list_statement(
    text: &str,
    line: &str,
    line_offset: usize,
) -> Option<(ExtractedStatement, usize)> {
    let start_padding = line.len() - line.trim_start().len();
    let trimmed = line.trim_start();
    let name = parse_list_name(trimmed)?;

    let mut items = Vec::new();
    let mut next_offset = next_line_offset(text, line_offset)?;
    let mut end = line_offset + line.len();

    while next_offset <= text.len() {
        let next_end = text[next_offset..]
            .find('\n')
            .map(|idx| next_offset + idx)
            .unwrap_or(text.len());
        let next_raw_line = &text[next_offset..next_end];
        let next_line = next_raw_line.trim_end_matches('\r');

        if next_line.trim().is_empty() {
            end = next_offset.saturating_sub(1);
            break;
        }

        items.push(normalize_list_item(next_line));
        end = next_end;

        if next_end == text.len() {
            next_offset = text.len();
            break;
        }

        next_offset = next_end + 1;
    }

    Some((
        ExtractedStatement {
            text: format!("{name} := [{}]", items.join(", ")),
            source_span: Span::new(line_offset + start_padding, end),
            display_span: Span::new(line_offset + start_padding, end),
            insert_span: Span::new(end, end),
        },
        next_offset,
    ))
}

fn parse_list_name(line: &str) -> Option<&str> {
    let name = line.strip_suffix(':')?;
    if name.is_empty() {
        return None;
    }

    let mut chars = name.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    if chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        Some(name)
    } else {
        None
    }
}

fn next_line_offset(text: &str, line_offset: usize) -> Option<usize> {
    let line_end = text[line_offset..]
        .find('\n')
        .map(|idx| line_offset + idx)
        .unwrap_or(text.len());
    (line_end != text.len()).then_some(line_end + 1)
}

fn normalize_list_item(line: &str) -> String {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("- ") {
        return rest.trim_start().to_string();
    }
    if let Some(rest) = trimmed.strip_prefix("* ") {
        return rest.trim_start().to_string();
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_only_lines_after_marker() {
        let text = "before\nmath:\na := 1\nb := a + 2\n";
        let statements = extract_statements(text);

        assert_eq!(statements.len(), 2);
        assert_eq!(statements[0].text, "a := 1");
        assert_eq!(statements[1].text, "b := a + 2");
    }

    #[test]
    fn supports_expression_on_marker_line() {
        let text = "math: 2 + 2\n3 + 3";
        let statements = extract_statements(text);

        assert_eq!(statements.len(), 2);
        assert_eq!(statements[0].text, "2 + 2");
        assert_eq!(statements[1].text, "3 + 3");
    }

    #[test]
    fn requires_marker_at_start_of_line() {
        let text = "  math:\na := 1\nmath:\nb := 2\n";
        let statements = extract_statements(text);

        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].text, "b := 2");
    }

    #[test]
    fn collects_column_list_blocks() {
        let text = "math:\nfigures:\n12\n18\n9\n21\n\nsum(figures)\n";
        let statements = extract_statements(text);

        assert_eq!(statements.len(), 2);
        assert_eq!(statements[0].text, "figures := [12, 18, 9, 21]");
        assert_eq!(statements[1].text, "sum(figures)");
    }

    #[test]
    fn list_blocks_can_end_at_end_of_file() {
        let text = "math:\nfigures:\n12\n18";
        let statements = extract_statements(text);

        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].text, "figures := [12, 18]");
    }

    #[test]
    fn collects_markdown_bullet_list_blocks() {
        let text = "math:\nfigures:\n- 12\n- 18\n* 9\n* 21\n\nsum(figures)\n";
        let statements = extract_statements(text);

        assert_eq!(statements.len(), 2);
        assert_eq!(statements[0].text, "figures := [12, 18, 9, 21]");
        assert_eq!(statements[1].text, "sum(figures)");
    }

    #[test]
    fn list_blocks_stop_at_blank_line() {
        let text = "math:\nprices:\n98.99\n17.09\n\n11.55\nsum(prices)\n";
        let statements = extract_statements(text);

        assert_eq!(statements.len(), 3);
        assert_eq!(statements[0].text, "prices := [98.99, 17.09]");
        assert_eq!(statements[1].text, "11.55");
        assert_eq!(statements[2].text, "sum(prices)");
    }

    #[test]
    fn list_blocks_accept_expression_items() {
        let text = "math:\nprices:\n10\n5 * 3\n2 + 8\n";
        let statements = extract_statements(text);

        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].text, "prices := [10, 5 * 3, 2 + 8]");
    }
}

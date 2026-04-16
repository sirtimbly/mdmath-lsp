use std::collections::HashMap;

use crate::lang::{self, Expr, Stmt};
use crate::text::Span;

#[derive(Clone, Debug)]
pub struct ExtractedStatement {
    pub text: String,
    pub analysis_text: String,
    pub source_span: Span,
    pub display_span: Span,
    pub insert_span: Span,
    pub visible: bool,
}

#[derive(Clone, Copy)]
enum Mode {
    Math,
    Sheet,
}

#[derive(Clone)]
struct TableCell {
    text: String,
    span: Span,
}

struct TableColumn {
    letter: String,
    header_name: Option<String>,
    values: Vec<String>,
}

#[derive(Clone, Copy)]
struct FenceMarker {
    ch: u8,
    len: usize,
}

pub fn extract_statements(text: &str) -> Vec<ExtractedStatement> {
    let mut statements = Vec::new();
    let mut mode = None;
    let mut fence: Option<FenceMarker> = None;
    let mut offset = 0usize;

    while offset <= text.len() {
        let line_end = text[offset..]
            .find('\n')
            .map(|idx| offset + idx)
            .unwrap_or(text.len());
        let raw_line = &text[offset..line_end];
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim_start();

        if let Some(marker) = parse_fence_marker(trimmed) {
            match fence {
                Some(active) if active.ch == marker.ch && marker.len >= active.len => fence = None,
                None => fence = Some(marker),
                _ => {}
            }

            if line_end == text.len() {
                break;
            }
            offset = line_end + 1;
            continue;
        }

        if fence.is_some() {
            if line_end == text.len() {
                break;
            }
            offset = line_end + 1;
            continue;
        }

        if is_mode_terminator(line, mode) {
            mode = None;
        } else if let Some(rest) = line.strip_prefix("math:") {
            mode = Some(Mode::Math);
            push_marker_statement(&mut statements, line, rest, offset);
        } else if line.strip_prefix("sheet:").is_some() {
            mode = Some(Mode::Sheet);
        } else {
            match mode {
                Some(Mode::Math) => {
                    if let Some((statement, next_offset)) =
                        collect_list_statement(text, line, offset)
                    {
                        statements.push(statement);
                        offset = next_offset;
                        continue;
                    }
                    push_math_statement(&mut statements, line, offset);
                }
                Some(Mode::Sheet) => {
                    if let Some((table_statements, next_offset)) =
                        collect_sheet_table(text, line, offset)
                    {
                        statements.extend(table_statements);
                        offset = next_offset;
                        continue;
                    }
                    push_math_statement(&mut statements, line, offset);
                }
                None => {}
            }
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
    let visible_rest = strip_inline_code_spans(rest);
    let trimmed = visible_rest.trim_start();
    if trimmed.is_empty() {
        return;
    }

    let padding = visible_rest.len() - trimmed.len();
    let start = line_offset + "math:".len() + padding;
    let end = line_offset + line.len();

    statements.push(ExtractedStatement {
        text: trimmed.to_string(),
        analysis_text: strip_trailing_answer_text(trimmed),
        source_span: Span::new(start, end),
        display_span: Span::new(line_offset, end),
        insert_span: Span::new(end, end),
        visible: true,
    });
}

fn push_math_statement(statements: &mut Vec<ExtractedStatement>, line: &str, line_offset: usize) {
    if line.trim().is_empty() || is_wrapped_in_code_span(line.trim()) {
        return;
    }

    let start_padding = line.len() - line.trim_start().len();
    let start = line_offset + start_padding;
    let end = line_offset + line.len();
    let text = line.trim_start().to_string();
    let analysis_text = strip_trailing_answer_text(&strip_inline_code_spans(&text));
    if analysis_text.trim().is_empty() {
        return;
    }

    statements.push(ExtractedStatement {
        text: text.clone(),
        analysis_text,
        source_span: Span::new(start, end),
        display_span: Span::new(start, end),
        insert_span: Span::new(end, end),
        visible: true,
    });
}

fn collect_list_statement(
    text: &str,
    line: &str,
    line_offset: usize,
) -> Option<(ExtractedStatement, usize)> {
    let start_padding = line.len() - line.trim_start().len();
    let visible = strip_inline_code_spans(line.trim_start());
    let trimmed = visible.trim();
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

        if is_mode_terminator(next_line, Some(Mode::Math))
            || is_mode_terminator(next_line, Some(Mode::Sheet))
        {
            break;
        }

        let item = normalize_list_item(next_line);
        if item.is_empty() {
            if next_end == text.len() {
                next_offset = text.len();
                break;
            }
            next_offset = next_end + 1;
            continue;
        }

        items.push(item);
        end = next_end;

        if next_end == text.len() {
            next_offset = text.len();
            break;
        }

        next_offset = next_end + 1;
    }

    let statement = format!("{name} := [{}]", items.join(", "));
    Some((
        ExtractedStatement {
            text: statement.clone(),
            analysis_text: statement,
            source_span: Span::new(line_offset + start_padding, end),
            display_span: Span::new(line_offset + start_padding, end),
            insert_span: Span::new(end, end),
            visible: true,
        },
        next_offset,
    ))
}

fn collect_sheet_table(
    text: &str,
    line: &str,
    line_offset: usize,
) -> Option<(Vec<ExtractedStatement>, usize)> {
    let header_cells = parse_table_row(line, line_offset)?;
    let separator_offset = next_line_offset(text, line_offset)?;
    let separator_end = line_end(text, separator_offset);
    let separator_line = text[separator_offset..separator_end].trim_end_matches('\r');
    let separator_cells = parse_table_row(separator_line, separator_offset)?;

    if header_cells.len() != separator_cells.len() || !is_separator_row(&separator_cells) {
        return None;
    }

    let mut columns = header_cells
        .iter()
        .enumerate()
        .map(|(index, cell)| TableColumn {
            letter: column_label(index),
            header_name: normalize_header_name(&cell.text),
            values: Vec::new(),
        })
        .collect::<Vec<_>>();

    let mut statements = Vec::new();
    let mut next_offset = if separator_end == text.len() {
        text.len()
    } else {
        separator_end + 1
    };
    let mut row_index = 0usize;

    while next_offset <= text.len() {
        let row_end = line_end(text, next_offset);
        let row_line = text[next_offset..row_end].trim_end_matches('\r');
        let Some(cells) = parse_table_row(row_line, next_offset) else {
            break;
        };
        if cells.len() != columns.len() {
            break;
        }

        row_index += 1;
        statements.extend(extract_sheet_row_statements(
            line_offset,
            row_index,
            &cells,
            &mut columns,
        ));

        if row_end == text.len() {
            next_offset = text.len();
            break;
        }
        next_offset = row_end + 1;
    }

    for column in columns {
        let Some(header_name) = column.header_name else {
            continue;
        };
        if column.values.is_empty() {
            continue;
        }

        let assignment = format!("{header_name} := [{}]", column.values.join(", "));
        statements.push(ExtractedStatement {
            text: assignment.clone(),
            analysis_text: assignment,
            source_span: Span::new(line_offset, line_offset),
            display_span: Span::new(line_offset, line_offset),
            insert_span: Span::new(line_offset, line_offset),
            visible: false,
        });
    }

    Some((statements, next_offset))
}

fn extract_sheet_row_statements(
    table_id: usize,
    row_index: usize,
    cells: &[TableCell],
    columns: &mut [TableColumn],
) -> Vec<ExtractedStatement> {
    let mut row_bindings = HashMap::new();

    for (column_index, cell) in cells.iter().enumerate() {
        let content = strip_inline_code_spans(cell.text.trim());
        let content = content.trim();
        if content.starts_with('=') || is_sheet_value_cell(content) {
            let hidden_name = hidden_cell_name(table_id, row_index, column_index);
            row_bindings.insert(columns[column_index].letter.clone(), hidden_name.clone());
            if let Some(header_name) = &columns[column_index].header_name {
                row_bindings.insert(header_name.clone(), hidden_name);
            }
        }
    }

    let mut statements = Vec::new();
    for (column_index, cell) in cells.iter().enumerate() {
        let visible = strip_inline_code_spans(cell.text.trim());
        let content = visible.trim();
        if content.is_empty() {
            continue;
        }

        let hidden_name = hidden_cell_name(table_id, row_index, column_index);

        if let Some(formula) = content.strip_prefix('=') {
            let rewritten = rewrite_formula_references(
                &strip_trailing_answer_text(formula.trim_start()),
                &row_bindings,
            );
            let hidden_assignment = format!("{hidden_name} := {rewritten}");
            statements.push(hidden_statement(cell.span, hidden_assignment));
            statements.push(ExtractedStatement {
                text: content.to_string(),
                analysis_text: rewritten,
                source_span: cell.span,
                display_span: cell.span,
                insert_span: Span::new(cell.span.end, cell.span.end),
                visible: true,
            });
            columns[column_index].values.push(hidden_name);
        } else if is_sheet_value_cell(content) {
            let hidden_assignment = format!("{hidden_name} := {content}");
            statements.push(hidden_statement(cell.span, hidden_assignment));
            columns[column_index].values.push(hidden_name);
        }
    }

    statements
}

fn hidden_statement(span: Span, analysis_text: String) -> ExtractedStatement {
    ExtractedStatement {
        text: analysis_text.clone(),
        analysis_text,
        source_span: span,
        display_span: span,
        insert_span: Span::new(span.end, span.end),
        visible: false,
    }
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

fn parse_table_row(line: &str, line_offset: usize) -> Option<Vec<TableCell>> {
    if !line.trim_start().starts_with('|') {
        return None;
    }

    let pipe_positions = line
        .match_indices('|')
        .map(|(idx, _)| idx)
        .collect::<Vec<_>>();
    if pipe_positions.len() < 2 {
        return None;
    }

    let mut cells = Vec::new();
    for window in pipe_positions.windows(2) {
        let left = window[0];
        let right = window[1];
        let raw = &line[left + 1..right];
        let leading = raw.len() - raw.trim_start().len();
        let trailing = raw.len() - raw.trim_end().len();
        let start = line_offset + left + 1 + leading;
        let end = (line_offset + right).saturating_sub(trailing).max(start);

        cells.push(TableCell {
            text: raw.trim().to_string(),
            span: Span::new(start, end),
        });
    }

    Some(cells)
}

fn is_separator_row(cells: &[TableCell]) -> bool {
    cells.iter().all(|cell| {
        let text = cell.text.trim();
        !text.is_empty() && text.contains('-') && text.chars().all(|ch| ch == '-' || ch == ':')
    })
}

fn is_sheet_value_cell(text: &str) -> bool {
    match lang::parse_statement(text) {
        Ok(Stmt::Expr(Expr::Var(..))) | Ok(Stmt::Assign { .. }) => false,
        Ok(_) => true,
        Err(_) => false,
    }
}

fn normalize_header_name(header: &str) -> Option<String> {
    let mut normalized = String::new();
    let mut last_was_separator = false;

    for ch in header.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            normalized.push(ch);
            last_was_separator = false;
        } else if !normalized.is_empty() && !last_was_separator {
            normalized.push('_');
            last_was_separator = true;
        }
    }

    while normalized.ends_with('_') {
        normalized.pop();
    }
    if normalized.is_empty() {
        return None;
    }
    if normalized
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_digit())
    {
        normalized.insert(0, '_');
    }

    Some(normalized)
}

fn hidden_cell_name(table_id: usize, row_index: usize, column_index: usize) -> String {
    format!(
        "__sheet_t{}_r{}_{}",
        table_id,
        row_index,
        column_label(column_index).to_lowercase()
    )
}

fn column_label(mut index: usize) -> String {
    let mut label = String::new();
    loop {
        let remainder = index % 26;
        label.push((b'A' + remainder as u8) as char);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    label.chars().rev().collect()
}

fn rewrite_formula_references(formula: &str, bindings: &HashMap<String, String>) -> String {
    let bytes = formula.as_bytes();
    let mut result = String::new();
    let mut idx = 0usize;

    while idx < bytes.len() {
        if matches!(bytes[idx], b'a'..=b'z' | b'A'..=b'Z' | b'_') {
            let start = idx;
            idx += 1;
            while idx < bytes.len()
                && matches!(bytes[idx], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_')
            {
                idx += 1;
            }

            let ident = &formula[start..idx];
            if let Some(binding) = bindings.get(ident) {
                result.push_str(binding);
            } else {
                result.push_str(ident);
            }
        } else {
            result.push(bytes[idx] as char);
            idx += 1;
        }
    }

    result
}

fn line_end(text: &str, line_offset: usize) -> usize {
    text[line_offset..]
        .find('\n')
        .map(|idx| line_offset + idx)
        .unwrap_or(text.len())
}

fn next_line_offset(text: &str, line_offset: usize) -> Option<usize> {
    let end = line_end(text, line_offset);
    (end != text.len()).then_some(end + 1)
}

fn normalize_list_item(line: &str) -> String {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("- ") {
        return strip_inline_code_spans(rest.trim_start())
            .trim()
            .to_string();
    }
    if let Some(rest) = trimmed.strip_prefix("* ") {
        return strip_inline_code_spans(rest.trim_start())
            .trim()
            .to_string();
    }
    strip_inline_code_spans(trimmed).trim().to_string()
}

fn is_mode_terminator(line: &str, mode: Option<Mode>) -> bool {
    let trimmed = line.trim();
    match mode {
        Some(Mode::Math) => trimmed == "/math",
        Some(Mode::Sheet) => trimmed == "/sheet",
        None => false,
    }
}

fn strip_inline_code_spans(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut result = String::new();
    let mut idx = 0usize;

    while idx < bytes.len() {
        if bytes[idx] == b'`' {
            let ticks = count_ticks(bytes, idx);
            idx += ticks;
            while idx < bytes.len() {
                if bytes[idx] == b'`' && count_ticks(bytes, idx) == ticks {
                    idx += ticks;
                    break;
                }
                idx += 1;
            }
            continue;
        }

        result.push(bytes[idx] as char);
        idx += 1;
    }

    result
}

fn is_wrapped_in_code_span(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.first() != Some(&b'`') {
        return false;
    }
    let ticks = count_ticks(bytes, 0);
    if ticks == 0 || bytes.len() <= ticks * 2 {
        return false;
    }
    bytes[bytes.len() - ticks..]
        .iter()
        .all(|byte| *byte == b'`')
}

fn count_ticks(bytes: &[u8], start: usize) -> usize {
    bytes[start..]
        .iter()
        .take_while(|byte| **byte == b'`')
        .count()
}

fn parse_fence_marker(trimmed: &str) -> Option<FenceMarker> {
    let bytes = trimmed.as_bytes();
    let ch = *bytes.first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }

    let len = bytes.iter().take_while(|byte| **byte == ch).count();
    (len >= 3).then_some(FenceMarker { ch, len })
}

fn strip_trailing_answer_text(line: &str) -> String {
    let trimmed = line.trim_end();
    if trimmed.starts_with('=') {
        return trimmed.to_string();
    }

    for (idx, ch) in trimmed.char_indices() {
        if ch != '=' {
            continue;
        }

        if trimmed[..idx].ends_with(':') {
            continue;
        }

        return trimmed[..idx].trim_end().to_string();
    }

    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_only_lines_after_math_marker() {
        let text = "before\nmath:\na := 1\nb := a + 2\n";
        let statements = extract_statements(text);

        assert_eq!(statements.len(), 2);
        assert_eq!(statements[0].text, "a := 1");
        assert_eq!(statements[1].text, "b := a + 2");
    }

    #[test]
    fn supports_expression_on_math_marker_line() {
        let text = "math: 2 + 2\n3 + 3";
        let statements = extract_statements(text);

        assert_eq!(statements.len(), 2);
        assert_eq!(statements[0].text, "2 + 2");
        assert_eq!(statements[1].text, "3 + 3");
    }

    #[test]
    fn ignores_inserted_answer_text_in_math_lines() {
        let text = "math: 2 + 2 = 4\n3 + 3 = 6";
        let statements = extract_statements(text);

        assert_eq!(statements.len(), 2);
        assert_eq!(statements[0].text, "2 + 2 = 4");
        assert_eq!(statements[0].analysis_text, "2 + 2");
        assert_eq!(statements[1].analysis_text, "3 + 3");
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
    fn extracts_sheet_table_formulas() {
        let text = "sheet:\n| Item | Price | qty. | Total |\n| ---- | ----- | ---- | ----- |\n| MacBook Pro | 1999 | 2 | =sum(B,qty) |\n| iPad | 999 | 3 | =sum(Price,C) |\n\nsum(Price)\n";
        let statements = extract_statements(text);

        let visible = statements
            .iter()
            .filter(|statement| statement.visible)
            .collect::<Vec<_>>();
        assert_eq!(visible.len(), 3);
        assert_eq!(visible[0].text, "=sum(B,qty)");
        assert_eq!(
            visible[0].analysis_text,
            "sum(__sheet_t7_r1_b,__sheet_t7_r1_c)"
        );
        assert_eq!(visible[1].text, "=sum(Price,C)");
        assert_eq!(
            visible[1].analysis_text,
            "sum(__sheet_t7_r2_b,__sheet_t7_r2_c)"
        );
        assert_eq!(visible[2].text, "sum(Price)");
    }

    #[test]
    fn ignores_inserted_answer_text_in_sheet_formulas() {
        let text = "sheet:\n| Item | Price | qty. | Total |\n| ---- | ----- | ---- | ----- |\n| MacBook Pro | 1999 | 2 | =sum(B,qty) = 2001 |\n";
        let statements = extract_statements(text);

        let visible = statements
            .iter()
            .filter(|statement| statement.visible)
            .collect::<Vec<_>>();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].text, "=sum(B,qty) = 2001");
        assert_eq!(
            visible[0].analysis_text,
            "sum(__sheet_t7_r1_b,__sheet_t7_r1_c)"
        );
    }

    #[test]
    fn ignores_markers_inside_fenced_code_blocks() {
        let text = "```md\nmath:\n2 + 2\n```\nmath:\n3 + 3\n";
        let statements = extract_statements(text);

        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].analysis_text, "3 + 3");
    }

    #[test]
    fn ignores_fenced_code_blocks_inside_active_mode() {
        let text = "math:\na := 10\n```\nignored := 99\n```\na + 1\n";
        let statements = extract_statements(text);

        assert_eq!(statements.len(), 2);
        assert_eq!(statements[0].analysis_text, "a := 10");
        assert_eq!(statements[1].analysis_text, "a + 1");
    }

    #[test]
    fn strips_inline_code_spans_from_math_statements() {
        let text = "math:\na := 10 `comment`\na + 1\n";
        let statements = extract_statements(text);

        assert_eq!(statements.len(), 2);
        assert_eq!(statements[0].analysis_text, "a := 10");
        assert_eq!(statements[1].analysis_text, "a + 1");
    }

    #[test]
    fn ignores_lines_wrapped_in_code_spans_inside_math_mode() {
        let text = "math:\n`sum(prices)`\n2 + 2\n";
        let statements = extract_statements(text);

        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].analysis_text, "2 + 2");
    }

    #[test]
    fn explicit_math_terminator_closes_mode() {
        let text = "math:\n2 + 2\n/math\n3 + 3\n";
        let statements = extract_statements(text);

        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].analysis_text, "2 + 2");
    }

    #[test]
    fn explicit_sheet_terminator_closes_mode() {
        let text = "sheet:\n| Item | Price | qty. | Total |\n| ---- | ----- | ---- | ----- |\n| MacBook Pro | 1999 | 2 | =sum(B,qty) |\n/sheet\nsum(Price)\n";
        let statements = extract_statements(text);

        let visible = statements
            .iter()
            .filter(|statement| statement.visible)
            .collect::<Vec<_>>();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].text, "=sum(B,qty)");
    }

    #[test]
    fn explicit_terminator_stops_column_list_collection() {
        let text = "math:\nprices:\n98.99\n17.09\n/math\n11.55\n";
        let statements = extract_statements(text);

        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].analysis_text, "prices := [98.99, 17.09]");
    }
}

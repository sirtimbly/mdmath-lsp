use std::collections::HashMap;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::lang::{self, LangError, Stmt, Value};
use crate::markdown::{extract_statements, ExtractedStatement};
use crate::text::{span_to_range, Span};

pub struct Analysis {
    pub diagnostics: Vec<DiagnosticData>,
    statements: Vec<StatementAnalysis>,
}

pub struct DiagnosticData {
    pub span: Span,
    pub message: String,
}

#[derive(Clone)]
pub struct StatementAnalysis {
    source: ExtractedStatement,
    hint_label: Option<String>,
    replacement_text: Option<String>,
    hover_text: Option<String>,
}

pub fn analyze(text: &str) -> Analysis {
    let sources = extract_statements(text);
    let mut parsed = Vec::with_capacity(sources.len());
    let mut diagnostics = Vec::new();
    let mut statements = Vec::with_capacity(sources.len());

    for source in &sources {
        match lang::parse_statement(&source.text) {
            Ok(statement) => parsed.push(Some(statement)),
            Err(error) => {
                diagnostics.push(to_document_diagnostic(source, &error));
                parsed.push(None);
            }
        }

        statements.push(StatementAnalysis {
            source: source.clone(),
            hint_label: None,
            replacement_text: None,
            hover_text: None,
        });
    }

    if !sources.is_empty() {
        let scope = (0..sources.len()).collect::<Vec<_>>();
        evaluate_scope(&scope, &sources, &parsed, &mut statements, &mut diagnostics);
    }

    Analysis {
        diagnostics,
        statements,
    }
}

impl Analysis {
    pub fn statements(&self) -> &[StatementAnalysis] {
        &self.statements
    }

    pub fn statement_at_offset(&self, offset: usize) -> Option<&StatementAnalysis> {
        self.statements
            .iter()
            .find(|statement| statement.source.display_span.contains(offset))
    }
}

impl DiagnosticData {
    pub fn to_lsp(&self, text: &str) -> Diagnostic {
        Diagnostic {
            range: span_to_range(text, self.span),
            severity: Some(DiagnosticSeverity::ERROR),
            message: self.message.clone(),
            ..Diagnostic::default()
        }
    }
}

impl StatementAnalysis {
    pub fn source_span(&self) -> Span {
        self.source.source_span
    }

    pub fn display_span(&self) -> Span {
        self.source.display_span
    }

    pub fn insert_span(&self) -> Span {
        self.source.insert_span
    }

    pub fn hint_label(&self) -> Option<String> {
        self.hint_label.clone()
    }

    pub fn replacement_text(&self) -> Option<String> {
        self.replacement_text.clone()
    }

    pub fn hover_text(&self) -> Option<String> {
        self.hover_text.clone()
    }
}

fn evaluate_scope(
    scope: &[usize],
    sources: &[ExtractedStatement],
    parsed: &[Option<Stmt>],
    statements: &mut [StatementAnalysis],
    diagnostics: &mut Vec<DiagnosticData>,
) {
    if scope.is_empty() {
        return;
    }

    let mut evaluator = ScopeEvaluator::new(scope, parsed);
    for (scope_pos, global_idx) in scope.iter().copied().enumerate() {
        let Some(statement) = parsed[global_idx].as_ref() else {
            continue;
        };

        match evaluator.eval_statement(scope_pos) {
            Ok(value) => {
                statements[global_idx].hint_label = Some(hint_label(statement, &value));
                statements[global_idx].replacement_text = Some(lang::format_value(&value));
                statements[global_idx].hover_text =
                    Some(hover_text(&sources[global_idx].text, statement, &value));
            }
            Err(error) => diagnostics.push(to_document_diagnostic(&sources[global_idx], &error)),
        }
    }
}

struct ScopeEvaluator<'a> {
    scope: &'a [usize],
    parsed: &'a [Option<Stmt>],
    assignments: HashMap<String, Vec<usize>>,
    state: Vec<EvalState>,
}

#[derive(Clone)]
enum EvalState {
    Unvisited,
    Visiting,
    Done(Result<Value, LangError>),
}

impl<'a> ScopeEvaluator<'a> {
    fn new(scope: &'a [usize], parsed: &'a [Option<Stmt>]) -> Self {
        let mut assignments: HashMap<String, Vec<usize>> = HashMap::new();
        for (scope_pos, global_idx) in scope.iter().copied().enumerate() {
            if let Some(Stmt::Assign { name, .. }) = parsed[global_idx].as_ref() {
                assignments.entry(name.clone()).or_default().push(scope_pos);
            }
        }

        Self {
            scope,
            parsed,
            assignments,
            state: vec![EvalState::Unvisited; scope.len()],
        }
    }

    fn eval_statement(&mut self, scope_pos: usize) -> Result<Value, LangError> {
        match self.state[scope_pos].clone() {
            EvalState::Done(result) => return result,
            EvalState::Visiting => {
                let global_idx = self.scope[scope_pos];
                let span = self.parsed[global_idx]
                    .as_ref()
                    .map(Stmt::span)
                    .unwrap_or(Span::new(0, 0));
                return Err(LangError {
                    span,
                    message: "circular reference detected".to_string(),
                });
            }
            EvalState::Unvisited => {}
        }

        self.state[scope_pos] = EvalState::Visiting;
        let global_idx = self.scope[scope_pos];
        let statement = self.parsed[global_idx].as_ref().unwrap().clone();

        let result = {
            let mut resolver =
                |name: &str, span: Span| self.resolve_variable(name, span, scope_pos);
            lang::eval_statement(&statement, &mut resolver)
        };

        self.state[scope_pos] = EvalState::Done(result.clone());
        result
    }

    fn resolve_variable(
        &mut self,
        name: &str,
        span: Span,
        current_scope_pos: usize,
    ) -> Result<Value, LangError> {
        let Some(bindings) = self.assignments.get(name) else {
            return Err(LangError {
                span,
                message: format!("unknown variable `{name}`"),
            });
        };

        let candidate = bindings
            .iter()
            .copied()
            .filter(|binding| *binding <= current_scope_pos)
            .last()
            .or_else(|| bindings.first().copied())
            .ok_or_else(|| LangError {
                span,
                message: format!("unknown variable `{name}`"),
            })?;

        self.eval_statement(candidate).map_err(|error| LangError {
            span,
            message: error.message,
        })
    }
}

fn to_document_diagnostic(source: &ExtractedStatement, error: &LangError) -> DiagnosticData {
    DiagnosticData {
        span: Span::new(
            source.source_span.start + error.span.start,
            source.source_span.start + error.span.end,
        ),
        message: error.message.clone(),
    }
}

fn hint_label(statement: &Stmt, value: &Value) -> String {
    match statement {
        Stmt::Assign { name, .. } => format!("{name} = {}", lang::format_value(value)),
        _ => format!("= {}", lang::format_value(value)),
    }
}

fn hover_text(source: &str, statement: &Stmt, value: &Value) -> String {
    let summary = match statement {
        Stmt::Assign { name, .. } => format!("{name} = {}", lang::format_value(value)),
        _ => lang::format_value(value),
    };

    format!("Expression: `{}`\n\nResult: `{summary}`", source.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(text: &str) -> Vec<String> {
        analyze(text)
            .statements
            .iter()
            .filter_map(|statement| statement.hint_label.clone())
            .collect()
    }

    fn diagnostic_messages(text: &str) -> Vec<String> {
        analyze(text)
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect()
    }

    #[test]
    fn evaluates_arithmetic() {
        let labels = labels("math:\n2 + 2 * 5");
        assert_eq!(labels, vec!["= 12"]);
    }

    #[test]
    fn evaluates_variables_in_document_order() {
        let labels = labels("math:\na := 10\nb := a * 2\na + b");
        assert_eq!(labels, vec!["a = 10", "b = 20", "= 30"]);
    }

    #[test]
    fn evaluates_lists() {
        let labels = labels("math:\nnums := [1, 2, 3, 4]\nsum(nums)\navg(nums)");
        assert_eq!(labels, vec!["nums = [1, 2, 3, 4]", "= 10", "= 2.5"]);
    }

    #[test]
    fn evaluates_conversion() {
        let labels = labels("math:\n5 ft -> m");
        assert_eq!(labels, vec!["= 1.524 m"]);
    }

    #[test]
    fn reports_unknown_variable() {
        let analysis = analyze("math:\na + 1");
        assert_eq!(analysis.diagnostics[0].message, "unknown variable `a`");
    }

    #[test]
    fn reports_bad_list_function_arguments() {
        let analysis = analyze("math:\nsum(3)");
        assert_eq!(analysis.diagnostics[0].message, "expected list argument");
    }

    #[test]
    fn reports_incompatible_conversion() {
        let analysis = analyze("math:\n5 ft -> kg");
        assert_eq!(
            analysis.diagnostics[0].message,
            "incompatible conversion from `ft` to `kg`"
        );
    }

    #[test]
    fn ignores_lines_before_marker() {
        let text = "plain text\nmore text\nmath:\na := 10\na + 1";
        let labels = labels(text);
        assert_eq!(labels, vec!["a = 10", "= 11"]);
    }

    #[test]
    fn evaluates_column_lists() {
        let text = "math:\nfigures:\n12\n18\n9\n21\n\nsum(figures)\navg(figures)";
        let labels = labels(text);
        assert_eq!(labels, vec!["figures = [12, 18, 9, 21]", "= 60", "= 15"]);
    }

    #[test]
    fn evaluates_markdown_bullet_lists() {
        let text = "math:\nfigures:\n- 12\n- 18\n- 9\n- 21\n\nsum(figures)\navg(figures)";
        let labels = labels(text);
        assert_eq!(labels, vec!["figures = [12, 18, 9, 21]", "= 60", "= 15"]);
    }

    #[test]
    fn evaluates_expression_items_inside_column_lists() {
        let text = "math:\nbase := 10\nprices:\nbase\nbase * 2\n5 + 5\n\nsum(prices)\navg(prices)";
        let labels = labels(text);
        assert_eq!(
            labels,
            vec![
                "base = 10",
                "prices = [10, 20, 10]",
                "= 40",
                "= 13.3333333333"
            ]
        );
    }

    #[test]
    fn evaluates_min_max_and_len_for_column_lists() {
        let text =
            "math:\nprices:\n- 98.99\n- 17.09\n- 11.55\n\nmin(prices)\nmax(prices)\nlen(prices)";
        let labels = labels(text);
        assert_eq!(
            labels,
            vec![
                "prices = [98.99, 17.09, 11.55]",
                "= 11.55",
                "= 98.99",
                "= 3"
            ]
        );
    }

    #[test]
    fn blank_line_ends_column_list_before_following_expression() {
        let text = "math:\nprices:\n98.99\n17.09\n\n11.55\nsum(prices)";
        let labels = labels(text);
        assert_eq!(
            labels,
            vec!["prices = [98.99, 17.09]", "= 11.55", "= 116.08"]
        );
    }

    #[test]
    fn reports_invalid_characters_in_column_list_items() {
        let messages = diagnostic_messages("math:\nprices:\n$98.99\n17.09\n\nsum(prices)");
        assert!(messages
            .iter()
            .any(|message| message.contains("unexpected character `$`")));
        assert!(messages
            .iter()
            .any(|message| message.contains("unknown variable `prices`")));
    }

    #[test]
    fn cycles_are_flagged() {
        let analysis = analyze("math:\na := b + 1\nb := a + 1");
        assert!(analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("circular reference")));
    }
}

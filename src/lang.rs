use crate::text::Span;

#[derive(Clone, Debug)]
pub enum Expr {
    Number(f64, Span),
    Var(String, Span),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
        span: Span,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    List(Vec<Expr>, Span),
    Call {
        name: String,
        args: Vec<Expr>,
        span: Span,
    },
    Quantity {
        expr: Box<Expr>,
        unit: String,
        span: Span,
    },
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Assign {
        name: String,
        expr: Expr,
        span: Span,
    },
    Convert {
        expr: Expr,
        to_unit: String,
        to_unit_span: Span,
        span: Span,
    },
    Expr(Expr),
}

#[derive(Clone, Copy, Debug)]
pub enum UnaryOp {
    Neg,
}

#[derive(Clone, Copy, Debug)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f64),
    List(Vec<f64>),
    Quantity { value: f64, unit: String },
}

#[derive(Clone, Debug)]
pub struct LangError {
    pub span: Span,
    pub message: String,
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Number(_, span)
            | Expr::Var(_, span)
            | Expr::List(_, span)
            | Expr::Unary { span, .. }
            | Expr::Binary { span, .. }
            | Expr::Call { span, .. }
            | Expr::Quantity { span, .. } => *span,
        }
    }
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Stmt::Assign { span, .. } | Stmt::Convert { span, .. } => *span,
            Stmt::Expr(expr) => expr.span(),
        }
    }
}

#[derive(Clone, Debug)]
struct Token {
    kind: TokenKind,
    span: Span,
}

#[derive(Clone, Debug, PartialEq)]
enum TokenKind {
    Number(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Assign,
    Arrow,
}

pub fn parse_statement(source: &str) -> Result<Stmt, LangError> {
    let tokens = lex(source)?;
    let mut parser = Parser { tokens, idx: 0 };
    let statement = parser.parse_statement()?;
    if let Some(token) = parser.peek() {
        return Err(LangError {
            span: token.span,
            message: "unexpected trailing input".to_string(),
        });
    }
    Ok(statement)
}

pub fn eval_statement(
    statement: &Stmt,
    resolver: &mut dyn FnMut(&str, Span) -> Result<Value, LangError>,
) -> Result<Value, LangError> {
    match statement {
        Stmt::Assign { expr, .. } | Stmt::Expr(expr) => eval_expr(expr, resolver),
        Stmt::Convert {
            expr,
            to_unit,
            to_unit_span,
            ..
        } => {
            let value = eval_expr(expr, resolver)?;
            let Value::Quantity {
                value: amount,
                unit: from_unit,
            } = value
            else {
                return Err(LangError {
                    span: expr.span(),
                    message: "expected a quantity for unit conversion".to_string(),
                });
            };

            let converted =
                convert_quantity(amount, &from_unit, to_unit).map_err(|message| LangError {
                    span: *to_unit_span,
                    message,
                })?;

            Ok(Value::Quantity {
                value: converted,
                unit: to_unit.clone(),
            })
        }
    }
}

pub fn format_value(value: &Value) -> String {
    match value {
        Value::Number(number) => format_number(*number),
        Value::List(items) => {
            let parts = items
                .iter()
                .map(|item| format_number(*item))
                .collect::<Vec<_>>();
            format!("[{}]", parts.join(", "))
        }
        Value::Quantity { value, unit } => format!("{} {}", format_number(*value), unit),
    }
}

fn eval_expr(
    expr: &Expr,
    resolver: &mut dyn FnMut(&str, Span) -> Result<Value, LangError>,
) -> Result<Value, LangError> {
    match expr {
        Expr::Number(number, _) => Ok(Value::Number(*number)),
        Expr::Var(name, span) => resolver(name, *span),
        Expr::Unary { op, expr, span } => {
            let value = eval_expr(expr, resolver)?;
            let number = expect_number(value, *span)?;
            match op {
                UnaryOp::Neg => Ok(Value::Number(-number)),
            }
        }
        Expr::Binary {
            op, left, right, ..
        } => {
            let left_span = left.span();
            let right_span = right.span();
            let left = expect_number(eval_expr(left, resolver)?, left_span)?;
            let right = expect_number(eval_expr(right, resolver)?, right_span)?;

            let value = match op {
                BinaryOp::Add => left + right,
                BinaryOp::Sub => left - right,
                BinaryOp::Mul => left * right,
                BinaryOp::Div => {
                    if right == 0.0 {
                        return Err(LangError {
                            span: right_span,
                            message: "divide by zero".to_string(),
                        });
                    }
                    left / right
                }
                BinaryOp::Pow => left.powf(right),
            };

            Ok(Value::Number(value))
        }
        Expr::List(items, _) => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                values.push(expect_number(eval_expr(item, resolver)?, item.span())?);
            }
            if items.is_empty() {
                return Ok(Value::List(Vec::new()));
            }
            Ok(Value::List(values))
        }
        Expr::Call { name, args, span } => eval_call(name, args, *span, resolver),
        Expr::Quantity { expr, unit, span } => {
            let amount = expect_number(eval_expr(expr, resolver)?, expr.span())?;
            if unit_definition(unit).is_none() {
                return Err(LangError {
                    span: *span,
                    message: format!("unknown unit `{unit}`"),
                });
            }
            Ok(Value::Quantity {
                value: amount,
                unit: unit.clone(),
            })
        }
    }
}

fn eval_call(
    name: &str,
    args: &[Expr],
    span: Span,
    resolver: &mut dyn FnMut(&str, Span) -> Result<Value, LangError>,
) -> Result<Value, LangError> {
    match name {
        "sum" => Ok(Value::Number(
            eval_numeric_args(args, span, resolver)?.iter().sum(),
        )),
        "avg" => {
            let values = eval_numeric_args(args, span, resolver)?;
            if values.is_empty() {
                return Err(LangError {
                    span,
                    message: "`avg` requires a non-empty list".to_string(),
                });
            }
            Ok(Value::Number(
                values.iter().sum::<f64>() / values.len() as f64,
            ))
        }
        "min" => reduce_numeric_args(args, span, resolver, f64::min, "min"),
        "max" => reduce_numeric_args(args, span, resolver, f64::max, "max"),
        "len" => eval_single_list_fn(
            args,
            span,
            resolver,
            |list| Ok(Value::Number(list.len() as f64)),
            "len",
        ),
        "count" => {
            let values = eval_numeric_args(args, span, resolver)?;
            Ok(Value::Number(values.len() as f64))
        }
        "product" => Ok(Value::Number(
            eval_numeric_args(args, span, resolver)?
                .into_iter()
                .product(),
        )),
        "median" => {
            let mut values = eval_numeric_args(args, span, resolver)?;
            if values.is_empty() {
                return Err(LangError {
                    span,
                    message: "`median` requires a non-empty list".to_string(),
                });
            }
            values.sort_by(f64::total_cmp);
            let mid = values.len() / 2;
            let median = if values.len() % 2 == 0 {
                (values[mid - 1] + values[mid]) / 2.0
            } else {
                values[mid]
            };
            Ok(Value::Number(median))
        }
        "abs" => eval_unary_number_fn(args, span, resolver, f64::abs, "abs"),
        "round" => eval_round(args, span, resolver),
        "floor" => eval_unary_number_fn(args, span, resolver, f64::floor, "floor"),
        "ceil" => eval_unary_number_fn(args, span, resolver, f64::ceil, "ceil"),
        "sqrt" => eval_unary_number_fn(args, span, resolver, f64::sqrt, "sqrt"),
        _ => Err(LangError {
            span,
            message: format!("unknown function `{name}`"),
        }),
    }
}

fn eval_numeric_args(
    args: &[Expr],
    span: Span,
    resolver: &mut dyn FnMut(&str, Span) -> Result<Value, LangError>,
) -> Result<Vec<f64>, LangError> {
    if args.is_empty() {
        return Err(LangError {
            span,
            message: "expected at least one argument".to_string(),
        });
    }

    if args.len() == 1 {
        let value = eval_expr(&args[0], resolver)?;
        return match value {
            Value::List(items) => Ok(items),
            other => Ok(vec![expect_number(other, args[0].span())?]),
        };
    }

    let mut values = Vec::with_capacity(args.len());
    for arg in args {
        values.push(expect_number(eval_expr(arg, resolver)?, arg.span())?);
    }
    Ok(values)
}

fn reduce_numeric_args(
    args: &[Expr],
    span: Span,
    resolver: &mut dyn FnMut(&str, Span) -> Result<Value, LangError>,
    reducer: fn(f64, f64) -> f64,
    name: &str,
) -> Result<Value, LangError> {
    let values = eval_numeric_args(args, span, resolver)?;
    values
        .iter()
        .copied()
        .reduce(reducer)
        .map(Value::Number)
        .ok_or_else(|| LangError {
            span,
            message: format!("`{name}` requires a non-empty list"),
        })
}

fn eval_single_list_fn(
    args: &[Expr],
    span: Span,
    resolver: &mut dyn FnMut(&str, Span) -> Result<Value, LangError>,
    callback: impl FnOnce(Vec<f64>) -> Result<Value, LangError>,
    name: &str,
) -> Result<Value, LangError> {
    if args.len() != 1 {
        return Err(LangError {
            span,
            message: format!("`{name}` expects exactly one argument"),
        });
    }

    let value = eval_expr(&args[0], resolver)?;
    callback(expect_list(value, args[0].span())?)
}

fn eval_unary_number_fn(
    args: &[Expr],
    span: Span,
    resolver: &mut dyn FnMut(&str, Span) -> Result<Value, LangError>,
    callback: fn(f64) -> f64,
    name: &str,
) -> Result<Value, LangError> {
    if args.len() != 1 {
        return Err(LangError {
            span,
            message: format!("`{name}` expects exactly one argument"),
        });
    }

    let value = expect_number(eval_expr(&args[0], resolver)?, args[0].span())?;
    Ok(Value::Number(callback(value)))
}

fn eval_round(
    args: &[Expr],
    span: Span,
    resolver: &mut dyn FnMut(&str, Span) -> Result<Value, LangError>,
) -> Result<Value, LangError> {
    if !(1..=2).contains(&args.len()) {
        return Err(LangError {
            span,
            message: "`round` expects one or two arguments".to_string(),
        });
    }

    let value = expect_number(eval_expr(&args[0], resolver)?, args[0].span())?;
    let digits = if args.len() == 2 {
        expect_number(eval_expr(&args[1], resolver)?, args[1].span())?
    } else {
        0.0
    };
    let factor = 10f64.powf(digits);
    Ok(Value::Number((value * factor).round() / factor))
}

fn expect_number(value: Value, span: Span) -> Result<f64, LangError> {
    match value {
        Value::Number(number) => Ok(number),
        Value::List(_) => Err(LangError {
            span,
            message: "expected number, found list".to_string(),
        }),
        Value::Quantity { .. } => Err(LangError {
            span,
            message: "expected number, found quantity".to_string(),
        }),
    }
}

fn expect_list(value: Value, span: Span) -> Result<Vec<f64>, LangError> {
    match value {
        Value::List(items) => Ok(items),
        Value::Number(_) | Value::Quantity { .. } => Err(LangError {
            span,
            message: "expected list argument".to_string(),
        }),
    }
}

fn lex(source: &str) -> Result<Vec<Token>, LangError> {
    let mut tokens = Vec::new();
    let bytes = source.as_bytes();
    let mut idx = 0usize;

    while idx < bytes.len() {
        match bytes[idx] {
            b' ' | b'\t' | b'\r' | b'\n' => idx += 1,
            b'0'..=b'9' | b'.' => {
                let start = idx;
                let mut seen_dot = bytes[idx] == b'.';
                idx += 1;
                while idx < bytes.len() {
                    match bytes[idx] {
                        b'0'..=b'9' => idx += 1,
                        b'.' if !seen_dot => {
                            seen_dot = true;
                            idx += 1;
                        }
                        _ => break,
                    }
                }
                let text = &source[start..idx];
                let number = text.parse::<f64>().map_err(|_| LangError {
                    span: Span::new(start, idx),
                    message: format!("invalid number `{text}`"),
                })?;
                tokens.push(Token {
                    kind: TokenKind::Number(number),
                    span: Span::new(start, idx),
                });
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                let start = idx;
                idx += 1;
                while idx < bytes.len()
                    && matches!(bytes[idx], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_')
                {
                    idx += 1;
                }
                tokens.push(Token {
                    kind: TokenKind::Ident(source[start..idx].to_string()),
                    span: Span::new(start, idx),
                });
            }
            b'+' => {
                tokens.push(Token {
                    kind: TokenKind::Plus,
                    span: Span::new(idx, idx + 1),
                });
                idx += 1;
            }
            b'-' => {
                if idx + 1 < bytes.len() && bytes[idx + 1] == b'>' {
                    tokens.push(Token {
                        kind: TokenKind::Arrow,
                        span: Span::new(idx, idx + 2),
                    });
                    idx += 2;
                } else {
                    tokens.push(Token {
                        kind: TokenKind::Minus,
                        span: Span::new(idx, idx + 1),
                    });
                    idx += 1;
                }
            }
            b'*' => {
                tokens.push(Token {
                    kind: TokenKind::Star,
                    span: Span::new(idx, idx + 1),
                });
                idx += 1;
            }
            b'/' => {
                tokens.push(Token {
                    kind: TokenKind::Slash,
                    span: Span::new(idx, idx + 1),
                });
                idx += 1;
            }
            b'^' => {
                tokens.push(Token {
                    kind: TokenKind::Caret,
                    span: Span::new(idx, idx + 1),
                });
                idx += 1;
            }
            b'(' => {
                tokens.push(Token {
                    kind: TokenKind::LParen,
                    span: Span::new(idx, idx + 1),
                });
                idx += 1;
            }
            b')' => {
                tokens.push(Token {
                    kind: TokenKind::RParen,
                    span: Span::new(idx, idx + 1),
                });
                idx += 1;
            }
            b'[' => {
                tokens.push(Token {
                    kind: TokenKind::LBracket,
                    span: Span::new(idx, idx + 1),
                });
                idx += 1;
            }
            b']' => {
                tokens.push(Token {
                    kind: TokenKind::RBracket,
                    span: Span::new(idx, idx + 1),
                });
                idx += 1;
            }
            b',' => {
                tokens.push(Token {
                    kind: TokenKind::Comma,
                    span: Span::new(idx, idx + 1),
                });
                idx += 1;
            }
            b':' if idx + 1 < bytes.len() && bytes[idx + 1] == b'=' => {
                tokens.push(Token {
                    kind: TokenKind::Assign,
                    span: Span::new(idx, idx + 2),
                });
                idx += 2;
            }
            _ => {
                return Err(LangError {
                    span: Span::new(idx, idx + 1),
                    message: format!("unexpected character `{}`", bytes[idx] as char),
                })
            }
        }
    }

    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    idx: usize,
}

impl Parser {
    fn parse_statement(&mut self) -> Result<Stmt, LangError> {
        if let (Some(TokenKind::Ident(name)), Some(TokenKind::Assign)) =
            (self.peek_kind(), self.peek_nth_kind(1))
        {
            let name = name.clone();
            let name_span = self.bump().unwrap().span;
            self.bump();
            let expr = self.parse_expr_with_optional_unit()?;
            let span = name_span.cover(expr.span());
            return Ok(Stmt::Assign { name, expr, span });
        }

        let expr = self.parse_expr_with_optional_unit()?;
        if matches!(self.peek_kind(), Some(TokenKind::Arrow)) {
            self.bump();
            let Some(token) = self.bump() else {
                return Err(LangError {
                    span: expr.span(),
                    message: "expected target unit after `->`".to_string(),
                });
            };
            let TokenKind::Ident(to_unit) = token.kind else {
                return Err(LangError {
                    span: token.span,
                    message: "expected target unit after `->`".to_string(),
                });
            };

            return Ok(Stmt::Convert {
                span: expr.span().cover(token.span),
                expr,
                to_unit,
                to_unit_span: token.span,
            });
        }

        Ok(Stmt::Expr(expr))
    }

    fn parse_expr_with_optional_unit(&mut self) -> Result<Expr, LangError> {
        let mut expr = self.parse_additive()?;
        if let Some(Token {
            kind: TokenKind::Ident(unit),
            span,
        }) = self.peek().cloned()
        {
            self.bump();
            expr = Expr::Quantity {
                span: expr.span().cover(span),
                expr: Box::new(expr),
                unit,
            };
        }
        Ok(expr)
    }

    fn parse_additive(&mut self) -> Result<Expr, LangError> {
        let mut expr = self.parse_multiplicative()?;
        loop {
            let op = match self.peek_kind() {
                Some(TokenKind::Plus) => BinaryOp::Add,
                Some(TokenKind::Minus) => BinaryOp::Sub,
                _ => break,
            };
            self.bump();
            let right = self.parse_multiplicative()?;
            expr = Expr::Binary {
                op,
                span: expr.span().cover(right.span()),
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, LangError> {
        let mut expr = self.parse_power()?;
        loop {
            let op = match self.peek_kind() {
                Some(TokenKind::Star) => BinaryOp::Mul,
                Some(TokenKind::Slash) => BinaryOp::Div,
                _ => break,
            };
            self.bump();
            let right = self.parse_power()?;
            expr = Expr::Binary {
                op,
                span: expr.span().cover(right.span()),
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_power(&mut self) -> Result<Expr, LangError> {
        let expr = self.parse_unary()?;
        if matches!(self.peek_kind(), Some(TokenKind::Caret)) {
            self.bump();
            let right = self.parse_power()?;
            return Ok(Expr::Binary {
                op: BinaryOp::Pow,
                span: expr.span().cover(right.span()),
                left: Box::new(expr),
                right: Box::new(right),
            });
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, LangError> {
        if matches!(self.peek_kind(), Some(TokenKind::Minus)) {
            let token = self.bump().unwrap();
            let expr = self.parse_unary()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Neg,
                span: token.span.cover(expr.span()),
                expr: Box::new(expr),
            });
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Expr, LangError> {
        let Some(token) = self.bump() else {
            return Err(LangError {
                span: Span::new(0, 0),
                message: "unexpected end of input".to_string(),
            });
        };

        match token.kind {
            TokenKind::Number(value) => Ok(Expr::Number(value, token.span)),
            TokenKind::Ident(name) => {
                if matches!(self.peek_kind(), Some(TokenKind::LParen)) {
                    self.bump();
                    let mut args = Vec::new();
                    if !matches!(self.peek_kind(), Some(TokenKind::RParen)) {
                        loop {
                            args.push(self.parse_additive()?);
                            if matches!(self.peek_kind(), Some(TokenKind::Comma)) {
                                self.bump();
                                continue;
                            }
                            break;
                        }
                    }
                    let Some(close) = self.bump() else {
                        return Err(LangError {
                            span: token.span,
                            message: "expected `)`".to_string(),
                        });
                    };
                    if close.kind != TokenKind::RParen {
                        return Err(LangError {
                            span: close.span,
                            message: "expected `)`".to_string(),
                        });
                    }
                    Ok(Expr::Call {
                        name,
                        args,
                        span: token.span.cover(close.span),
                    })
                } else {
                    Ok(Expr::Var(name, token.span))
                }
            }
            TokenKind::LParen => {
                let expr = self.parse_additive()?;
                let Some(close) = self.bump() else {
                    return Err(LangError {
                        span: token.span,
                        message: "expected `)`".to_string(),
                    });
                };
                if close.kind != TokenKind::RParen {
                    return Err(LangError {
                        span: close.span,
                        message: "expected `)`".to_string(),
                    });
                }
                Ok(expr)
            }
            TokenKind::LBracket => {
                let mut items = Vec::new();
                if !matches!(self.peek_kind(), Some(TokenKind::RBracket)) {
                    loop {
                        items.push(self.parse_additive()?);
                        if matches!(self.peek_kind(), Some(TokenKind::Comma)) {
                            self.bump();
                            continue;
                        }
                        break;
                    }
                }
                let Some(close) = self.bump() else {
                    return Err(LangError {
                        span: token.span,
                        message: "expected `]`".to_string(),
                    });
                };
                if close.kind != TokenKind::RBracket {
                    return Err(LangError {
                        span: close.span,
                        message: "expected `]`".to_string(),
                    });
                }
                Ok(Expr::List(items, token.span.cover(close.span)))
            }
            _ => Err(LangError {
                span: token.span,
                message: "expected expression".to_string(),
            }),
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.idx)
    }

    fn peek_kind(&self) -> Option<&TokenKind> {
        self.peek().map(|token| &token.kind)
    }

    fn peek_nth_kind(&self, offset: usize) -> Option<&TokenKind> {
        self.tokens.get(self.idx + offset).map(|token| &token.kind)
    }

    fn bump(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.idx).cloned();
        self.idx += usize::from(token.is_some());
        token
    }
}

#[derive(Clone, Copy)]
enum UnitKind {
    Linear { factor_to_base: f64 },
    Temperature(TemperatureScale),
}

#[derive(Clone, Copy, PartialEq)]
enum Dimension {
    Length,
    Mass,
    Time,
    Temperature,
}

#[derive(Clone, Copy)]
enum TemperatureScale {
    C,
    F,
    K,
}

#[derive(Clone, Copy)]
struct UnitDefinition {
    dimension: Dimension,
    kind: UnitKind,
}

fn unit_definition(unit: &str) -> Option<UnitDefinition> {
    let definition = match unit {
        "mm" => UnitDefinition {
            dimension: Dimension::Length,
            kind: UnitKind::Linear {
                factor_to_base: 0.001,
            },
        },
        "cm" => UnitDefinition {
            dimension: Dimension::Length,
            kind: UnitKind::Linear {
                factor_to_base: 0.01,
            },
        },
        "m" => UnitDefinition {
            dimension: Dimension::Length,
            kind: UnitKind::Linear {
                factor_to_base: 1.0,
            },
        },
        "km" => UnitDefinition {
            dimension: Dimension::Length,
            kind: UnitKind::Linear {
                factor_to_base: 1000.0,
            },
        },
        "in" => UnitDefinition {
            dimension: Dimension::Length,
            kind: UnitKind::Linear {
                factor_to_base: 0.0254,
            },
        },
        "ft" => UnitDefinition {
            dimension: Dimension::Length,
            kind: UnitKind::Linear {
                factor_to_base: 0.3048,
            },
        },
        "yd" => UnitDefinition {
            dimension: Dimension::Length,
            kind: UnitKind::Linear {
                factor_to_base: 0.9144,
            },
        },
        "mi" => UnitDefinition {
            dimension: Dimension::Length,
            kind: UnitKind::Linear {
                factor_to_base: 1609.344,
            },
        },
        "g" => UnitDefinition {
            dimension: Dimension::Mass,
            kind: UnitKind::Linear {
                factor_to_base: 1.0,
            },
        },
        "kg" => UnitDefinition {
            dimension: Dimension::Mass,
            kind: UnitKind::Linear {
                factor_to_base: 1000.0,
            },
        },
        "oz" => UnitDefinition {
            dimension: Dimension::Mass,
            kind: UnitKind::Linear {
                factor_to_base: 28.349523125,
            },
        },
        "lb" => UnitDefinition {
            dimension: Dimension::Mass,
            kind: UnitKind::Linear {
                factor_to_base: 453.59237,
            },
        },
        "s" => UnitDefinition {
            dimension: Dimension::Time,
            kind: UnitKind::Linear {
                factor_to_base: 1.0,
            },
        },
        "min" => UnitDefinition {
            dimension: Dimension::Time,
            kind: UnitKind::Linear {
                factor_to_base: 60.0,
            },
        },
        "h" => UnitDefinition {
            dimension: Dimension::Time,
            kind: UnitKind::Linear {
                factor_to_base: 3600.0,
            },
        },
        "C" => UnitDefinition {
            dimension: Dimension::Temperature,
            kind: UnitKind::Temperature(TemperatureScale::C),
        },
        "F" => UnitDefinition {
            dimension: Dimension::Temperature,
            kind: UnitKind::Temperature(TemperatureScale::F),
        },
        "K" => UnitDefinition {
            dimension: Dimension::Temperature,
            kind: UnitKind::Temperature(TemperatureScale::K),
        },
        _ => return None,
    };

    Some(definition)
}

fn convert_quantity(value: f64, from: &str, to: &str) -> Result<f64, String> {
    let Some(from_def) = unit_definition(from) else {
        return Err(format!("unknown unit `{from}`"));
    };
    let Some(to_def) = unit_definition(to) else {
        return Err(format!("unknown unit `{to}`"));
    };

    if from_def.dimension != to_def.dimension {
        return Err(format!("incompatible conversion from `{from}` to `{to}`"));
    }

    match (from_def.kind, to_def.kind) {
        (
            UnitKind::Linear {
                factor_to_base: from_factor,
            },
            UnitKind::Linear {
                factor_to_base: to_factor,
            },
        ) => Ok(value * from_factor / to_factor),
        (UnitKind::Temperature(from_scale), UnitKind::Temperature(to_scale)) => {
            let kelvin = match from_scale {
                TemperatureScale::C => value + 273.15,
                TemperatureScale::F => (value - 32.0) * 5.0 / 9.0 + 273.15,
                TemperatureScale::K => value,
            };
            let converted = match to_scale {
                TemperatureScale::C => kelvin - 273.15,
                TemperatureScale::F => (kelvin - 273.15) * 9.0 / 5.0 + 32.0,
                TemperatureScale::K => kelvin,
            };
            Ok(converted)
        }
        _ => Err(format!("incompatible conversion from `{from}` to `{to}`")),
    }
}

fn format_number(number: f64) -> String {
    let value = if number.abs() < 1e-10 { 0.0 } else { number };
    let mut formatted = format!("{value:.10}");
    while formatted.contains('.') && formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    formatted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(source: &str) -> Result<Value, LangError> {
        let stmt = parse_statement(source)?;
        eval_statement(&stmt, &mut |name, span| {
            Err(LangError {
                span,
                message: format!("unknown variable `{name}`"),
            })
        })
    }

    #[test]
    fn parses_operator_precedence() {
        let value = eval("2 + 2 * 5").unwrap();
        assert_eq!(value, Value::Number(12.0));
    }

    #[test]
    fn parses_unary_minus() {
        let value = eval("-2 + 5").unwrap();
        assert_eq!(value, Value::Number(3.0));
    }

    #[test]
    fn parses_lists_and_functions() {
        let value = eval("sum([1, 2, 3])").unwrap();
        assert_eq!(value, Value::Number(6.0));
    }

    #[test]
    fn supports_variadic_spreadsheet_style_functions() {
        assert_eq!(eval("sum(1999, 2)").unwrap(), Value::Number(2001.0));
        assert_eq!(eval("product(3, 4)").unwrap(), Value::Number(12.0));
        assert_eq!(eval("count(3, 4, 5)").unwrap(), Value::Number(3.0));
    }

    #[test]
    fn evaluates_min_max_and_len() {
        assert_eq!(eval("min([3, 1, 2])").unwrap(), Value::Number(1.0));
        assert_eq!(eval("max([3, 1, 2])").unwrap(), Value::Number(3.0));
        assert_eq!(eval("len([3, 1, 2])").unwrap(), Value::Number(3.0));
    }

    #[test]
    fn evaluates_additional_spreadsheet_functions() {
        assert_eq!(eval("median([1, 9, 3])").unwrap(), Value::Number(3.0));
        assert_eq!(eval("abs(-5)").unwrap(), Value::Number(5.0));
        assert_eq!(eval("round(3.14159, 2)").unwrap(), Value::Number(3.14));
        assert_eq!(eval("floor(3.9)").unwrap(), Value::Number(3.0));
        assert_eq!(eval("ceil(3.1)").unwrap(), Value::Number(4.0));
        assert_eq!(eval("sqrt(9)").unwrap(), Value::Number(3.0));
    }

    #[test]
    fn parses_assignments_and_conversion() {
        assert!(matches!(
            parse_statement("a := 10"),
            Ok(Stmt::Assign { .. })
        ));
        assert!(matches!(
            parse_statement("5 ft -> m"),
            Ok(Stmt::Convert { .. })
        ));
    }

    #[test]
    fn reports_divide_by_zero() {
        let error = eval("10 / 0").unwrap_err();
        assert_eq!(error.message, "divide by zero");
    }

    #[test]
    fn converts_units() {
        let value = eval("32 F -> C").unwrap();
        assert_eq!(
            value,
            Value::Quantity {
                value: 0.0,
                unit: "C".to_string(),
            }
        );
    }

    #[test]
    fn reports_unknown_function() {
        let error = eval("mystery([1, 2, 3])").unwrap_err();
        assert_eq!(error.message, "unknown function `mystery`");
    }

    #[test]
    fn reports_wrong_argument_count() {
        let error = eval("len([1, 2], [3, 4])").unwrap_err();
        assert_eq!(error.message, "`len` expects exactly one argument");
    }

    #[test]
    fn reports_unknown_unit() {
        let error = eval("5 parsecs -> m").unwrap_err();
        assert_eq!(error.message, "unknown unit `parsecs`");
    }
}

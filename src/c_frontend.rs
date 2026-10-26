//! Dual-Tier C-to-TAC Frontend Adapter
//!
//! Provides two tiers of lowering C source code into native TAC format:
//! - Tier 1 (Active Default): Native pure-Rust in-memory C-subset lowerer (zero external dependencies).
//! - Tier 2 (Strictly Dormant Backup): GCC GIMPLE bridge invoked only via explicit CLI `c-backup`.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::ir::OpCode;

// ============================================================================
// Helper: C Code Detection
// ============================================================================

/// Detects if the given source looks like C source code rather than native TAC.
pub fn looks_like_c_code(source: &str) -> bool {
    if source.contains("#include")
        || source.contains("int main")
        || source.contains("void main")
        || source.contains("scanf")
        || source.contains("printf")
    {
        return true;
    }
    for raw_line in source.lines() {
        let line = raw_line.trim();
        let code = line.split('#').next().unwrap_or("").trim();
        let code = code.split("//").next().unwrap_or("").trim();
        if code.ends_with(';') {
            return true;
        }
    }
    false
}

// ============================================================================
// Tier 1: Native Pure-Rust C-Subset Lowerer (Active Default)
// ============================================================================

/// Lowers C source code into native three-address code (TAC) format in-memory.
///
/// Handles:
/// - Preprocessor directives (`#include`, `#define`) and comments (`//`, `/* */`)
/// - `main` wrapper and block braces
/// - `scanf("%d...", &a, &b)` -> `read a`, `read b`
/// - `printf("...", expr)` -> evaluate expr -> `print res`
/// - Variable declarations & assignments (`int x = expr;`, `x = expr;`, compound assignments)
/// - Compound expression flattening respecting C operator precedence
/// - Basic control flow (`label:`, `goto label;`, `if (!x) goto label;`, `if (x == 0) goto label;`)
pub fn lower_c_to_tac(c_source: &str) -> Result<String, String> {
    let stripped = strip_comments_and_preprocessor(c_source);
    let mut tac_lines: Vec<String> = Vec::new();
    let mut temp_counter = find_highest_temp_index(&stripped);

    let statements = split_into_statements_and_labels(&stripped);

    for stmt in statements {
        let trimmed = stmt.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Label: "label:"
        if trimmed.ends_with(':') && !trimmed.contains('=') && !trimmed.contains(' ') {
            tac_lines.push(trimmed.to_string());
            continue;
        }

        // Control flow: goto
        if let Some(target) = trimmed.strip_prefix("goto ") {
            let label = target.trim().trim_end_matches(';').trim();
            tac_lines.push(format!("goto {label}"));
            continue;
        }

        // Control flow: if statements
        if trimmed.starts_with("if") {
            lower_if_statement(trimmed, &mut tac_lines, &mut temp_counter)?;
            continue;
        }

        // Return statement: ignore or strip
        if trimmed.starts_with("return") {
            continue;
        }

        // scanf(...)
        if trimmed.contains("scanf") {
            lower_scanf(trimmed, &mut tac_lines)?;
            continue;
        }

        // printf(...)
        if trimmed.contains("printf") {
            lower_printf(trimmed, &mut tac_lines, &mut temp_counter)?;
            continue;
        }

        // Variable declarations and assignments
        lower_declaration_or_assignment(trimmed, &mut tac_lines, &mut temp_counter)?;
    }

    if tac_lines.is_empty() {
        return Err("C source produced no TAC instructions".to_string());
    }

    Ok(tac_lines.join("\n"))
}

// ----------------------------------------------------------------------------
// Comment and Preprocessor Stripper
// ----------------------------------------------------------------------------

fn strip_comments_and_preprocessor(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut in_block_comment = false;

    while let Some(ch) = chars.next() {
        if in_block_comment {
            if ch == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block_comment = false;
                result.push(' ');
            }
            continue;
        }

        if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            in_block_comment = true;
            continue;
        }

        if ch == '/' && chars.peek() == Some(&'/') {
            chars.next();
            // Skip until newline
            for next_ch in chars.by_ref() {
                if next_ch == '\n' {
                    result.push('\n');
                    break;
                }
            }
            continue;
        }

        result.push(ch);
    }

    // Strip preprocessor lines (lines starting with '#')
    let mut filtered = String::with_capacity(result.len());
    for line in result.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        filtered.push_str(line);
        filtered.push('\n');
    }

    filtered
}

/// Finds the highest `tN` index in the source code to avoid temporary name collisions.
fn find_highest_temp_index(source: &str) -> usize {
    let mut max_idx = 0;
    for token in source.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if let Some(num_str) = token.strip_prefix('t') {
            if let Ok(idx) = num_str.parse::<usize>() {
                if idx > max_idx {
                    max_idx = idx;
                }
            }
        }
    }
    max_idx
}

// ----------------------------------------------------------------------------
// Statement and Label Splitter
// ----------------------------------------------------------------------------

fn split_into_statements_and_labels(source: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut string_char = '"';

    for ch in source.chars() {
        if in_string {
            current.push(ch);
            if ch == string_char {
                in_string = false;
            } else if ch == '\\' {
                // Next char escaped
            }
            continue;
        }

        match ch {
            '"' | '\'' => {
                in_string = true;
                string_char = ch;
                current.push(ch);
            }
            '{' | '}' => {
                let trimmed = current.trim();
                // Check if this was a function header like "int main()"
                if trimmed.contains("main") {
                    current.clear();
                } else if !trimmed.is_empty() {
                    statements.push(trimmed.to_string());
                    current.clear();
                }
            }
            ';' => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    statements.push(trimmed.to_string());
                }
                current.clear();
            }
            ':' => {
                let trimmed = current.trim();
                // If it's a label (valid identifier characters only)
                if !trimmed.is_empty()
                    && !trimmed.contains('=')
                    && !trimmed.contains(' ')
                    && !trimmed.contains('?')
                    && trimmed.chars().all(|c| c.is_alphanumeric() || c == '_')
                {
                    statements.push(format!("{trimmed}:"));
                    current.clear();
                } else {
                    current.push(':');
                }
            }
            '\n' | '\r' => {
                // If the line by itself was a label, push it
                let trimmed = current.trim();
                if trimmed.ends_with(':')
                    && !trimmed.contains('=')
                    && !trimmed.contains(' ')
                    && trimmed.trim_end_matches(':').chars().all(|c| c.is_alphanumeric() || c == '_')
                {
                    statements.push(trimmed.to_string());
                    current.clear();
                } else {
                    current.push(' ');
                }
            }
            _ => {
                current.push(ch);
            }
        }
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() && trimmed != "}" && trimmed != "{" {
        statements.push(trimmed.to_string());
    }

    statements
}

// ----------------------------------------------------------------------------
// Control Flow Lowerer
// ----------------------------------------------------------------------------

fn lower_if_statement(
    stmt: &str,
    tac_lines: &mut Vec<String>,
    temp_counter: &mut usize,
) -> Result<(), String> {
    // Examples:
    // if (!x) goto label
    // if (x == 0) goto label
    // if (!x) { goto label }
    // if_false x goto label
    let cleaned = stmt.replace('{', " ").replace('}', " ");
    let mut parts = cleaned.split_whitespace();
    let first = parts.next().unwrap_or("");
    if first == "if_false" {
        let cond = parts.next().ok_or("Expected condition for if_false")?;
        let _ = parts.next(); // "goto"
        let label = parts.next().ok_or("Expected label for if_false")?;
        tac_lines.push(format!("if_false {cond} goto {label}"));
        return Ok(());
    }

    // Extract inside of if (...)
    let open_paren = stmt.find('(').ok_or_else(|| format!("Expected '(' in if statement: {stmt}"))?;
    let close_paren = stmt.rfind(')').ok_or_else(|| format!("Expected ')' in if statement: {stmt}"))?;
    let cond_str = stmt[open_paren + 1..close_paren].trim();

    let rest = stmt[close_paren + 1..].trim();
    let rest_clean = rest.replace('{', " ").replace('}', " ");
    let goto_target = rest_clean
        .split_whitespace()
        .skip_while(|&tok| tok != "goto")
        .nth(1)
        .ok_or_else(|| format!("Expected 'goto label' in if branch: {stmt}"))?
        .trim_end_matches(';');

    // Handle condition:
    // Case 1: "!x" or "!(expr)"
    if let Some(inner) = cond_str.strip_prefix('!') {
        let inner = inner.trim().trim_start_matches('(').trim_end_matches(')').trim();
        let cond_val = if is_simple_operand(inner) {
            inner.to_string()
        } else {
            let expr = parse_expression(inner)?;
            emit_expression_tac(&expr, None, tac_lines, temp_counter)
        };
        tac_lines.push(format!("if_false {cond_val} goto {goto_target}"));
        return Ok(());
    }

    // Case 2: "x == 0" or "(expr) == 0"
    if cond_str.contains("==") {
        let mut sides = cond_str.split("==");
        let left = sides.next().unwrap_or("").trim();
        let right = sides.next().unwrap_or("").trim();
        if right == "0" {
            let cond_val = if is_simple_operand(left) {
                left.to_string()
            } else {
                let expr = parse_expression(left)?;
                emit_expression_tac(&expr, None, tac_lines, temp_counter)
            };
            tac_lines.push(format!("if_false {cond_val} goto {goto_target}"));
            return Ok(());
        } else if left == "0" {
            let cond_val = if is_simple_operand(right) {
                right.to_string()
            } else {
                let expr = parse_expression(right)?;
                emit_expression_tac(&expr, None, tac_lines, temp_counter)
            };
            tac_lines.push(format!("if_false {cond_val} goto {goto_target}"));
            return Ok(());
        }
    }

    // Fallback: evaluate condition, if condition is a simple variable/temp
    let cond_val = if is_simple_operand(cond_str) {
        cond_str.to_string()
    } else {
        let expr = parse_expression(cond_str)?;
        emit_expression_tac(&expr, None, tac_lines, temp_counter)
    };
    tac_lines.push(format!("if_false {cond_val} goto {goto_target}"));
    Ok(())
}

// ----------------------------------------------------------------------------
// I/O: scanf and printf Lowerers
// ----------------------------------------------------------------------------

fn lower_scanf(stmt: &str, tac_lines: &mut Vec<String>) -> Result<(), String> {
    // Example: scanf("%d %d", &a, &b)
    let open = stmt.find('(').ok_or_else(|| format!("Expected '(' in scanf: {stmt}"))?;
    let close = stmt.rfind(')').ok_or_else(|| format!("Expected ')' in scanf: {stmt}"))?;
    let inner = &stmt[open + 1..close];

    let parts = split_arguments(inner);
    if parts.is_empty() {
        return Ok(());
    }

    // Skip the format string (first argument)
    for arg in parts.iter().skip(1) {
        let var = arg.trim().trim_start_matches('&').trim();
        if !var.is_empty() {
            tac_lines.push(format!("read {var}"));
        }
    }

    Ok(())
}

fn lower_printf(
    stmt: &str,
    tac_lines: &mut Vec<String>,
    temp_counter: &mut usize,
) -> Result<(), String> {
    // Example: printf("%d\n", x) or printf("%d\n", a + b)
    let open = stmt.find('(').ok_or_else(|| format!("Expected '(' in printf: {stmt}"))?;
    let close = stmt.rfind(')').ok_or_else(|| format!("Expected ')' in printf: {stmt}"))?;
    let inner = &stmt[open + 1..close];

    let parts = split_arguments(inner);
    if parts.is_empty() {
        return Ok(());
    }

    // Arguments after the format string
    for arg in parts.iter().skip(1) {
        let expr_str = arg.trim();
        if expr_str.is_empty() {
            continue;
        }

        let val = if is_simple_operand(expr_str) {
            expr_str.to_string()
        } else {
            let expr = parse_expression(expr_str)?;
            emit_expression_tac(&expr, None, tac_lines, temp_counter)
        };
        tac_lines.push(format!("print {val}"));
    }

    Ok(())
}

fn split_arguments(args_str: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut depth = 0;

    for ch in args_str.chars() {
        match ch {
            '"' => {
                in_string = !in_string;
                current.push(ch);
            }
            '(' if !in_string => {
                depth += 1;
                current.push(ch);
            }
            ')' if !in_string => {
                depth -= 1;
                current.push(ch);
            }
            ',' if !in_string && depth == 0 => {
                args.push(current.trim().to_string());
                current.clear();
            }
            _ => {
                current.push(ch);
            }
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        args.push(trimmed.to_string());
    }
    args
}

// ----------------------------------------------------------------------------
// Declarations and Assignments Lowerer
// ----------------------------------------------------------------------------

fn lower_declaration_or_assignment(
    stmt: &str,
    tac_lines: &mut Vec<String>,
    temp_counter: &mut usize,
) -> Result<(), String> {
    let s = strip_type_qualifiers(stmt.trim());

    // Check for compound assignments: +=, -=, *=, /=, %=, <<=, >>=
    let compound_ops = [
        ("+=", "+"),
        ("-=", "-"),
        ("*=", "*"),
        ("/=", "/"),
        ("%=", "%"),
        ("<<=", "<<"),
        (">>=", ">>"),
    ];
    for (c_op, tac_op) in compound_ops {
        if let Some(pos) = s.find(c_op) {
            let var = s[..pos].trim();
            let rhs = s[pos + c_op.len()..].trim();
            let expr_str = format!("{var} {tac_op} ({rhs})");
            let expr = parse_expression(&expr_str)?;
            emit_expression_tac(&expr, Some(var), tac_lines, temp_counter);
            return Ok(());
        }
    }

    // Standard assignment: var = expr
    if s.contains('=') {
        // Handle comma-separated declarations if any, e.g. "int a = 1, b = 2"
        let parts = split_by_top_level_comma(&s);
        if parts.len() > 1 {
            for part in parts {
                lower_single_assignment(&part, tac_lines, temp_counter)?;
            }
            return Ok(());
        }
        lower_single_assignment(&s, tac_lines, temp_counter)?;
        return Ok(());
    }

    // Bare declaration without initialization (e.g. "int a;", "int a, b;")
    // Nothing to emit in TAC.
    Ok(())
}

fn lower_single_assignment(
    stmt: &str,
    tac_lines: &mut Vec<String>,
    temp_counter: &mut usize,
) -> Result<(), String> {
    let s = strip_type_qualifiers(stmt.trim());
    if let Some(eq_pos) = s.find('=') {
        let lhs = s[..eq_pos].trim();
        let rhs = s[eq_pos + 1..].trim();

        let var_name = lhs.split_whitespace().last().unwrap_or(lhs).trim();
        if var_name.is_empty() {
            return Err(format!("Missing LHS variable in assignment: {stmt}"));
        }

        let expr = parse_expression(rhs)?;
        emit_expression_tac(&expr, Some(var_name), tac_lines, temp_counter);
    }
    Ok(())
}

fn strip_type_qualifiers(s: &str) -> String {
    let type_keywords = [
        "int32_t",
        "uint32_t",
        "int",
        "long",
        "short",
        "unsigned",
        "signed",
        "auto",
        "const",
        "volatile",
    ];

    let mut words = s.split_whitespace().peekable();
    let mut stripped_words = Vec::new();
    let mut in_types = true;

    while let Some(word) = words.next() {
        if in_types && type_keywords.contains(&word) {
            continue;
        }
        in_types = false;
        stripped_words.push(word);
    }

    stripped_words.join(" ")
}

fn split_by_top_level_comma(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for ch in s.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                parts.push(current.trim().to_string());
                current.clear();
            }
            _ => {
                current.push(ch);
            }
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        parts.push(trimmed.to_string());
    }
    parts
}

// ============================================================================
// Expression Parser & Flattener (Precedence Climbing)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
enum ExprNode {
    Const(i32),
    Var(String),
    Binary {
        op: &'static str,
        left: Box<ExprNode>,
        right: Box<ExprNode>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Number(i32),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Shl,
    Shr,
    LParen,
    RParen,
}

fn tokenize_expression(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }

        if ch.is_ascii_digit() {
            let mut num_str = String::new();
            while let Some(&d) = chars.peek() {
                if d.is_ascii_digit() {
                    num_str.push(d);
                    chars.next();
                } else {
                    break;
                }
            }
            let val = num_str.parse::<i32>().map_err(|e| format!("Invalid integer `{num_str}`: {e}"))?;
            tokens.push(Token::Number(val));
            continue;
        }

        if ch.is_ascii_alphabetic() || ch == '_' {
            let mut ident = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_alphanumeric() || c == '_' {
                    ident.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            tokens.push(Token::Ident(ident));
            continue;
        }

        match ch {
            '+' => {
                tokens.push(Token::Plus);
                chars.next();
            }
            '-' => {
                tokens.push(Token::Minus);
                chars.next();
            }
            '*' => {
                tokens.push(Token::Star);
                chars.next();
            }
            '/' => {
                tokens.push(Token::Slash);
                chars.next();
            }
            '%' => {
                tokens.push(Token::Percent);
                chars.next();
            }
            '<' => {
                chars.next();
                if chars.peek() == Some(&'<') {
                    chars.next();
                    tokens.push(Token::Shl);
                } else {
                    return Err(format!("Unexpected character `<` in expression: {input}"));
                }
            }
            '>' => {
                chars.next();
                if chars.peek() == Some(&'>') {
                    chars.next();
                    tokens.push(Token::Shr);
                } else {
                    return Err(format!("Unexpected character `>` in expression: {input}"));
                }
            }
            '(' => {
                tokens.push(Token::LParen);
                chars.next();
            }
            ')' => {
                tokens.push(Token::RParen);
                chars.next();
            }
            _ => {
                return Err(format!("Unexpected character `{ch}` in expression: {input}"));
            }
        }
    }

    Ok(tokens)
}

fn parse_expression(input: &str) -> Result<ExprNode, String> {
    let tokens = tokenize_expression(input)?;
    if tokens.is_empty() {
        return Err(format!("Empty expression in: {input}"));
    }
    let mut index = 0;
    let node = parse_precedence(&tokens, &mut index, 0)?;
    if index < tokens.len() {
        return Err(format!("Unparsed tokens in expression `{input}` at token {:?}", tokens[index]));
    }
    Ok(node)
}

fn get_operator_info(token: &Token) -> Option<(&'static str, u8)> {
    // Precedence levels (higher binds tighter):
    // 1: <<, >>
    // 2: +, -
    // 3: *, /, %
    match token {
        Token::Shl => Some(("<<", 1)),
        Token::Shr => Some((">>", 1)),
        Token::Plus => Some(("+", 2)),
        Token::Minus => Some(("-", 2)),
        Token::Star => Some(("*", 3)),
        Token::Slash => Some(("/", 3)),
        Token::Percent => Some(("%", 3)),
        _ => None,
    }
}

fn parse_precedence(tokens: &[Token], index: &mut usize, min_prec: u8) -> Result<ExprNode, String> {
    let mut left = parse_primary(tokens, index)?;

    while *index < tokens.len() {
        let op_info = get_operator_info(&tokens[*index]);
        let Some((op_str, prec)) = op_info else {
            break;
        };

        if prec < min_prec {
            break;
        }

        *index += 1; // Consume operator
        let right = parse_precedence(tokens, index, prec + 1)?;
        left = ExprNode::Binary {
            op: op_str,
            left: Box::new(left),
            right: Box::new(right),
        };
    }

    Ok(left)
}

fn parse_primary(tokens: &[Token], index: &mut usize) -> Result<ExprNode, String> {
    if *index >= tokens.len() {
        return Err("Unexpected end of expression".to_string());
    }

    match &tokens[*index] {
        Token::Number(val) => {
            let v = *val;
            *index += 1;
            Ok(ExprNode::Const(v))
        }
        Token::Ident(name) => {
            let n = name.clone();
            *index += 1;
            Ok(ExprNode::Var(n))
        }
        Token::Plus => {
            // Unary plus (+x -> x)
            *index += 1;
            parse_primary(tokens, index)
        }
        Token::Minus => {
            // Unary minus (-x -> 0 - x, -5 -> -5)
            *index += 1;
            let primary = parse_primary(tokens, index)?;
            match primary {
                ExprNode::Const(c) => Ok(ExprNode::Const(-c)),
                other => Ok(ExprNode::Binary {
                    op: "-",
                    left: Box::new(ExprNode::Const(0)),
                    right: Box::new(other),
                }),
            }
        }
        Token::LParen => {
            *index += 1; // Consume '('
            let inner = parse_precedence(tokens, index, 0)?;
            if *index >= tokens.len() || tokens[*index] != Token::RParen {
                return Err("Missing closing ')' in expression".to_string());
            }
            *index += 1; // Consume ')'
            Ok(inner)
        }
        other => Err(format!("Unexpected token in expression: {:?}", other)),
    }
}

/// Recursively flattens an expression tree into sequential TAC instructions.
/// If `dest_override` is provided, the root node writes directly to it without creating an extra temporary.
fn emit_expression_tac(
    expr: &ExprNode,
    dest_override: Option<&str>,
    tac: &mut Vec<String>,
    temp_counter: &mut usize,
) -> String {
    match expr {
        ExprNode::Const(c) => {
            if let Some(dest) = dest_override {
                tac.push(format!("{dest} = {c}"));
                dest.to_string()
            } else {
                c.to_string()
            }
        }
        ExprNode::Var(v) => {
            if let Some(dest) = dest_override {
                tac.push(format!("{dest} = {v}"));
                dest.to_string()
            } else {
                v.clone()
            }
        }
        ExprNode::Binary { op, left, right } => {
            let left_str = emit_expression_tac(left, None, tac, temp_counter);
            let right_str = emit_expression_tac(right, None, tac, temp_counter);

            let dest = if let Some(d) = dest_override {
                d.to_string()
            } else {
                *temp_counter += 1;
                format!("t{}", *temp_counter)
            };

            tac.push(format!("{dest} = {left_str} {op} {right_str}"));
            dest
        }
    }
}

fn is_simple_operand(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.parse::<i32>().is_ok() {
        return true;
    }
    if !trimmed.is_empty()
        && trimmed.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !trimmed.chars().next().unwrap_or('0').is_numeric()
    {
        return true;
    }
    false
}

// ============================================================================
// Tier 2: GCC GIMPLE Bridge (Strictly Dormant Backup)
// ============================================================================

use std::sync::atomic::{AtomicUsize, Ordering};
static GIMPLE_TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
    let count = GIMPLE_TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{prefix}_{}_{}_{}", std::process::id(), nanos, count))
}

/// Invokes GCC to dump GIMPLE intermediate representation and converts it to TAC.
///
/// STRICTLY DORMANT: This function must never be called during standard execution,
/// cargo tests, or normal GUI usage. It is only called if the user explicitly runs
/// `cargo run -- c-backup <file.c>`.
pub fn lower_c_via_gcc_backup(c_file_path: &str) -> Result<String, String> {
    let path = Path::new(c_file_path);
    if !path.exists() {
        return Err(format!("File does not exist: `{c_file_path}`"));
    }

    let temp_dir = unique_temp_dir("codeopt_gimple");
    fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("Could not create temporary directory: {e}"))?;

    let dump_dir_path = format!("{}/", temp_dir.display());

    let output = Command::new("gcc")
        .arg("-O1")
        .arg("-finline-functions")
        .arg("-fdump-tree-optimized")
        .arg("-fdump-tree-gimple")
        .arg("-S")
        .arg(c_file_path)
        .arg("-o")
        .arg("/dev/null")
        .arg("-dumpdir")
        .arg(&dump_dir_path)
        .output()
        .map_err(|e| format!("Failed to execute `gcc`: {e}. Ensure GCC is installed."))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(format!("GCC compilation failed: {stderr}"));
    }

    // Find the generated .optimized or .gimple file
    let mut chosen_file = None;
    if let Ok(entries) = fs::read_dir(&temp_dir) {
        let files: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        // Prefer .optimized (where inlining and pointer elimination occur)
        for p in &files {
            if p.to_string_lossy().contains(".optimized") {
                chosen_file = Some(p.clone());
                break;
            }
        }
        if chosen_file.is_none() {
            for p in &files {
                if p.to_string_lossy().contains(".gimple") {
                    chosen_file = Some(p.clone());
                    break;
                }
            }
        }
    }

    let Some(dump_path) = chosen_file else {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err("GCC did not produce any intermediate dump file".to_string());
    };

    let dump_content = fs::read_to_string(&dump_path)
        .map_err(|e| format!("Failed to read GCC dump file: {e}"))?;

    let _ = fs::remove_dir_all(&temp_dir);

    // Parse intermediate statements inside main()
    parse_gimple_to_tac(&dump_content)
}

/// Lowers raw in-memory C source text to TAC via the dormant GCC backup compiler.
pub fn lower_c_source_via_gcc_backup(c_source: &str) -> Result<String, String> {
    let temp_dir = unique_temp_dir("codeopt_gimple_src");
    fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("Could not create temporary directory: {e}"))?;
    let temp_c_path = temp_dir.join("input.c");
    fs::write(&temp_c_path, c_source)
        .map_err(|e| format!("Failed to write temporary C source: {e}"))?;
    let res = lower_c_via_gcc_backup(&temp_c_path.to_string_lossy());
    let _ = fs::remove_dir_all(&temp_dir);
    res
}

/// Compiles C source code with GCC and executes the resulting binary with user stdin,
/// returning the integers printed to stdout.
pub fn run_c_via_gcc(c_source: &str, stdin_input: &str) -> Result<Vec<i32>, String> {
    let temp_dir = unique_temp_dir("codeopt_run_gcc");
    fs::create_dir_all(&temp_dir).map_err(|e| format!("Failed to create temp dir: {e}"))?;
    let src_path = temp_dir.join("main.c");
    let bin_path = temp_dir.join("main.out");
    fs::write(&src_path, c_source).map_err(|e| format!("Failed to write C source: {e}"))?;

    let compile = Command::new("gcc")
        .arg("-O1")
        .arg(&src_path)
        .arg("-o")
        .arg(&bin_path)
        .output()
        .map_err(|e| format!("GCC invocation failed: {e}"))?;

    if !compile.status.success() {
        let _ = fs::remove_dir_all(&temp_dir);
        let err = String::from_utf8_lossy(&compile.stderr);
        return Err(format!("GCC compilation failed: {err}"));
    }

    use std::io::Write;
    let mut attempts = 0;
    let mut child = loop {
        match Command::new(&bin_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => break c,
            Err(_e) if attempts < 5 => {
                attempts += 1;
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
            Err(e) => {
                let _ = fs::remove_dir_all(&temp_dir);
                return Err(format!("Failed to spawn native executable: {e}"));
            }
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(stdin_input.as_bytes());
        let _ = stdin.write_all(b"\n");
    }

    let output = child.wait_with_output().map_err(|e| format!("Execution failed: {e}"))?;
    let _ = fs::remove_dir_all(&temp_dir);

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let numbers: Vec<i32> = stdout_str
        .split_whitespace()
        .filter_map(|s| s.parse::<i32>().ok())
        .collect();

    Ok(numbers)
}

fn clean_gimple_label(raw: &str) -> String {
    let trimmed = raw.trim();
    let name = if let (Some(open), Some(close)) = (trimmed.find('<'), trimmed.find('>')) {
        if close > open {
            &trimmed[open + 1..close]
        } else {
            trimmed
        }
    } else {
        trimmed.split([';', ':', '[', ' ', '\t']).next().unwrap_or(trimmed)
    };

    let mut cleaned: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect();

    // Strip leading or trailing underscores
    cleaned = cleaned.trim_matches('_').to_string();

    if cleaned.is_empty() {
        cleaned = "bb_target".to_string();
    }
    // If it starts with a digit, prefix with "bb_"
    if cleaned.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        cleaned = format!("bb_{cleaned}");
    }
    cleaned
}

fn extract_goto_target(s: &str) -> Option<String> {
    if let Some(pos) = s.find("goto") {
        let after = s[pos + 4..].trim();
        let lbl = clean_gimple_label(after);
        if !lbl.is_empty() {
            return Some(lbl);
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelOp {
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Ne,
}

pub fn parse_relational_expr(expr: &str) -> (Option<RelOp>, String, String) {
    let normalized = expr
        .replace("< =", "<=")
        .replace("> =", ">=")
        .replace("= =", "==")
        .replace("! =", "!=");
    let trimmed = normalized.trim();
    for (op_str, op) in &[
        ("<=", RelOp::Le),
        (">=", RelOp::Ge),
        ("==", RelOp::Eq),
        ("!=", RelOp::Ne),
        ("<", RelOp::Lt),
        (">", RelOp::Gt),
    ] {
        if let Some(pos) = trimmed.find(op_str) {
            let left = trimmed[..pos].trim();
            let right = trimmed[pos + op_str.len()..].trim();
            return (Some(*op), left.to_string(), right.to_string());
        }
    }
    (None, trimmed.to_string(), "0".to_string())
}

fn emit_relational_branch(
    op: Option<RelOp>,
    left: &str,
    right: &str,
    current_label: &str,
    then_target: &str,
    else_target: &str,
    edge_assignments: &HashMap<(String, String), Vec<(String, String)>>,
    temp_count: &mut usize,
    tac_lines: &mut Vec<String>,
    defined_labels: &mut HashSet<String>,
    referenced_labels: &mut HashSet<String>,
) {
    let then_edges = edge_assignments.get(&(current_label.to_string(), then_target.to_string()));
    let else_edges = edge_assignments.get(&(current_label.to_string(), else_target.to_string()));

    let has_then_edges = then_edges.map_or(false, |v| !v.is_empty());
    let has_else_edges = else_edges.map_or(false, |v| !v.is_empty());

    match op {
        Some(RelOp::Eq) => {
            *temp_count += 1;
            let t_diff = format!("t{}", *temp_count);
            tac_lines.push(format!("{t_diff} = {left} - {right}"));

            if has_then_edges {
                let trampoline = format!("bb_edge_{}_{}", current_label, then_target);
                referenced_labels.insert(trampoline.clone());
                tac_lines.push(format!("if_false {t_diff} goto {trampoline}"));

                if let Some(edges) = else_edges {
                    for (d, v) in edges {
                        tac_lines.push(format!("{d} = {v}"));
                    }
                }
                referenced_labels.insert(else_target.to_string());
                tac_lines.push(format!("goto {else_target}"));

                defined_labels.insert(trampoline.clone());
                tac_lines.push(format!("{trampoline}:"));
                if let Some(edges) = then_edges {
                    for (d, v) in edges {
                        tac_lines.push(format!("{d} = {v}"));
                    }
                }
                referenced_labels.insert(then_target.to_string());
                tac_lines.push(format!("goto {then_target}"));
            } else {
                referenced_labels.insert(then_target.to_string());
                tac_lines.push(format!("if_false {t_diff} goto {then_target}"));

                if let Some(edges) = else_edges {
                    for (d, v) in edges {
                        tac_lines.push(format!("{d} = {v}"));
                    }
                }
                referenced_labels.insert(else_target.to_string());
                tac_lines.push(format!("goto {else_target}"));
            }
        }
        _ => {
            *temp_count += 1;
            let t_diff = format!("t{}", *temp_count);
            *temp_count += 1;
            let t_sign = format!("t{}", *temp_count);
            *temp_count += 1;
            let t_cond = format!("t{}", *temp_count);

            let is_inverted = match op {
                Some(RelOp::Lt) => {
                    tac_lines.push(format!("{t_diff} = {left} - {right}"));
                    tac_lines.push(format!("{t_sign} = {t_diff} >> 31"));
                    tac_lines.push(format!("{t_cond} = 0 - {t_sign}"));
                    false
                }
                Some(RelOp::Gt) => {
                    tac_lines.push(format!("{t_diff} = {right} - {left}"));
                    tac_lines.push(format!("{t_sign} = {t_diff} >> 31"));
                    tac_lines.push(format!("{t_cond} = 0 - {t_sign}"));
                    false
                }
                Some(RelOp::Le) => {
                    tac_lines.push(format!("{t_diff} = {right} - {left}"));
                    tac_lines.push(format!("{t_sign} = {t_diff} >> 31"));
                    *temp_count += 1;
                    let t_gt = format!("t{}", *temp_count);
                    tac_lines.push(format!("{t_gt} = 0 - {t_sign}"));
                    tac_lines.push(format!("{t_cond} = 1 - {t_gt}"));
                    false
                }
                Some(RelOp::Ge) => {
                    tac_lines.push(format!("{t_diff} = {left} - {right}"));
                    tac_lines.push(format!("{t_sign} = {t_diff} >> 31"));
                    *temp_count += 1;
                    let t_lt = format!("t{}", *temp_count);
                    tac_lines.push(format!("{t_lt} = 0 - {t_sign}"));
                    tac_lines.push(format!("{t_cond} = 1 - {t_lt}"));
                    false
                }
                Some(RelOp::Ne) => {
                    tac_lines.push(format!("{t_cond} = {left} - {right}"));
                    false
                }
                Some(RelOp::Eq) => unreachable!("handled in outer match"),
                None => {
                    if let Some(inner) = left.strip_prefix('!') {
                        tac_lines.push(format!("{t_cond} = {inner}"));
                        true
                    } else {
                        tac_lines.push(format!("{t_cond} = {left}"));
                        false
                    }
                }
            };

            let (jump_target, fall_target, jump_edges, fall_edges, has_jump_edges) = if is_inverted {
                (then_target, else_target, then_edges, else_edges, has_then_edges)
            } else {
                (else_target, then_target, else_edges, then_edges, has_else_edges)
            };

            if has_jump_edges {
                let trampoline = format!("bb_edge_{}_{}", current_label, jump_target);
                referenced_labels.insert(trampoline.clone());
                tac_lines.push(format!("if_false {t_cond} goto {trampoline}"));

                if let Some(edges) = fall_edges {
                    for (d, v) in edges {
                        tac_lines.push(format!("{d} = {v}"));
                    }
                }
                referenced_labels.insert(fall_target.to_string());
                tac_lines.push(format!("goto {fall_target}"));

                defined_labels.insert(trampoline.clone());
                tac_lines.push(format!("{trampoline}:"));
                if let Some(edges) = jump_edges {
                    for (d, v) in edges {
                        tac_lines.push(format!("{d} = {v}"));
                    }
                }
                referenced_labels.insert(jump_target.to_string());
                tac_lines.push(format!("goto {jump_target}"));
            } else {
                referenced_labels.insert(jump_target.to_string());
                tac_lines.push(format!("if_false {t_cond} goto {jump_target}"));

                if let Some(edges) = fall_edges {
                    for (d, v) in edges {
                        tac_lines.push(format!("{d} = {v}"));
                    }
                }
                referenced_labels.insert(fall_target.to_string());
                tac_lines.push(format!("goto {fall_target}"));
            }
        }
    }
}

fn emit_relational_assignment(
    lhs: &str,
    op: RelOp,
    left: &str,
    right: &str,
    temp_count: &mut usize,
    tac_lines: &mut Vec<String>,
) {
    *temp_count += 1;
    let t_diff = format!("t{}", *temp_count);
    *temp_count += 1;
    let t_sign = format!("t{}", *temp_count);

    match op {
        RelOp::Lt => {
            tac_lines.push(format!("{t_diff} = {left} - {right}"));
            tac_lines.push(format!("{t_sign} = {t_diff} >> 31"));
            tac_lines.push(format!("{lhs} = 0 - {t_sign}"));
        }
        RelOp::Gt => {
            tac_lines.push(format!("{t_diff} = {right} - {left}"));
            tac_lines.push(format!("{t_sign} = {t_diff} >> 31"));
            tac_lines.push(format!("{lhs} = 0 - {t_sign}"));
        }
        RelOp::Le => {
            tac_lines.push(format!("{t_diff} = {right} - {left}"));
            tac_lines.push(format!("{t_sign} = {t_diff} >> 31"));
            *temp_count += 1;
            let t_gt = format!("t{}", *temp_count);
            tac_lines.push(format!("{t_gt} = 0 - {t_sign}"));
            tac_lines.push(format!("{lhs} = 1 - {t_gt}"));
        }
        RelOp::Ge => {
            tac_lines.push(format!("{t_diff} = {left} - {right}"));
            tac_lines.push(format!("{t_sign} = {t_diff} >> 31"));
            *temp_count += 1;
            let t_lt = format!("t{}", *temp_count);
            tac_lines.push(format!("{t_lt} = 0 - {t_sign}"));
            tac_lines.push(format!("{lhs} = 1 - {t_lt}"));
        }
        RelOp::Eq => {
            *temp_count += 1;
            let lbl_eq = format!("bb_eq_{}", *temp_count);
            let lbl_end = format!("bb_end_{}", *temp_count);
            tac_lines.push(format!("{lhs} = 0"));
            tac_lines.push(format!("{t_diff} = {left} - {right}"));
            tac_lines.push(format!("if_false {t_diff} goto {lbl_eq}"));
            tac_lines.push(format!("goto {lbl_end}"));
            tac_lines.push(format!("{lbl_eq}:"));
            tac_lines.push(format!("{lhs} = 1"));
            tac_lines.push(format!("{lbl_end}:"));
        }
        RelOp::Ne => {
            *temp_count += 1;
            let lbl_eq = format!("bb_eq_{}", *temp_count);
            let lbl_end = format!("bb_end_{}", *temp_count);
            tac_lines.push(format!("{lhs} = 1"));
            tac_lines.push(format!("{t_diff} = {left} - {right}"));
            tac_lines.push(format!("if_false {t_diff} goto {lbl_eq}"));
            tac_lines.push(format!("goto {lbl_end}"));
            tac_lines.push(format!("{lbl_eq}:"));
            tac_lines.push(format!("{lhs} = 0"));
            tac_lines.push(format!("{lbl_end}:"));
        }
    }
}

fn clean_gimple_token(
    raw: &str,
    temp_map: &mut HashMap<String, String>,
    temp_count: &mut usize,
    pointer_map: &HashMap<String, String>,
) -> String {
    let mut token = raw.trim();

    // Strip type casts like (int), (long unsigned int), (sizetype), (int *)
    while token.starts_with('(') {
        if let Some(close_paren) = token.find(')') {
            token = token[close_paren + 1..].trim();
        } else {
            break;
        }
    }

    // Pointer dereference: *a or *_1 or *a_3(D)
    if let Some(inner) = token.strip_prefix('*') {
        let clean_inner = clean_gimple_token(inner, temp_map, temp_count, pointer_map);
        if let Some(target) = pointer_map.get(&clean_inner) {
            return target.clone();
        }
        return clean_inner;
    }

    // Address-of: &x
    if let Some(inner) = token.strip_prefix('&') {
        return clean_gimple_token(inner, temp_map, temp_count, pointer_map);
    }

    // MEM expressions: MEM[(int *)_121] or MEM <int[100]> [(int *)&a][_7]
    if token.starts_with("MEM") {
        if let Some(last_open) = token.rfind('[') {
            if let Some(last_close) = token.rfind(']') {
                if last_close > last_open {
                    let inner = &token[last_open + 1..last_close];
                    let cleaned_inner = clean_gimple_token(inner, temp_map, temp_count, pointer_map);
                    if !cleaned_inner.is_empty() {
                        return format!("mem_{cleaned_inner}");
                    }
                }
            }
        }
        *temp_count += 1;
        return format!("t{}", *temp_count);
    }

    // Strip SSA default definition annotations: e.g. a_3(D) -> a_3
    if token.ends_with("(D)") {
        token = token[..token.len() - 3].trim();
    }

    // Array indexing: arr[0] -> arr_0
    if token.contains('[') && token.contains(']') {
        let cleaned = token.replace('[', "_").replace(']', "");
        return cleaned;
    }

    // Check if it's already a clean integer constant (including negative and wrapped u32)
    if let Ok(val) = token.parse::<i32>() {
        return val.to_string();
    }
    if let Ok(val) = token.parse::<u32>() {
        return (val as i32).to_string();
    }

    // Compiler temporaries: _1, x.1_2, D.3440, ivtmp.27_54, etc.
    if token.starts_with('_') || token.contains('.') || token.contains('#') {
        if let Some(mapped) = temp_map.get(token) {
            return mapped.clone();
        }
        *temp_count += 1;
        let fresh = format!("t{}", *temp_count);
        temp_map.insert(token.to_string(), fresh.clone());
        return fresh;
    }

    // Final sanity check: retain only valid identifier characters
    let sanitized: String = token
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();

    if sanitized.is_empty() {
        *temp_count += 1;
        format!("t{}", *temp_count)
    } else {
        sanitized
    }
}

#[derive(Debug, Clone)]
struct GimpleRawBlock {
    label: String,
    phis: Vec<(String, String, String)>,
    stmts: Vec<String>,
}

fn lower_mem_expr(
    raw: &str,
    temp_map: &mut HashMap<String, String>,
    temp_count: &mut usize,
    pointer_map: &HashMap<String, String>,
    tac_lines: &mut Vec<String>,
) -> String {
    let s = raw.trim();

    // Case 1: unoptimized array indexing: arr[idx]
    if !s.starts_with("MEM") && s.contains('[') && s.ends_with(']') {
        if let Some(open_b) = s.find('[') {
            let base = s[..open_b].trim();
            let idx = s[open_b + 1..s.len() - 1].trim();
            let c_base = clean_gimple_token(base, temp_map, temp_count, pointer_map);
            let c_idx = clean_gimple_token(idx, temp_map, temp_count, pointer_map);

            *temp_count += 1;
            let t_sc = format!("t{}", *temp_count);
            tac_lines.push(format!("{t_sc} = {c_idx} * 4"));

            *temp_count += 1;
            let t_ad = format!("t{}", *temp_count);
            tac_lines.push(format!("{t_ad} = {c_base} + {t_sc}"));

            return format!("mem_{t_ad}");
        }
    }

    // Case 2: MEM[...]
    if s.starts_with("MEM") {
        if let (Some(ob), Some(cb)) = (s.rfind('['), s.rfind(']')) {
            if cb > ob {
                let mut inner = s[ob + 1..cb].trim();
                while inner.starts_with('(') {
                    if let Some(cp) = inner.find(')') {
                        inner = inner[cp + 1..].trim();
                    } else {
                        break;
                    }
                }

                if inner.contains(" + ") {
                    let parts: Vec<&str> = inner.split(" + ").collect();
                    let base = parts[0].trim().trim_start_matches('&');
                    let rest = parts[1].trim();
                    let c_base = clean_gimple_token(base, temp_map, temp_count, pointer_map);

                    if rest.contains(" * ") {
                        let mult: Vec<&str> = rest.split(" * ").collect();
                        let idx = mult[0].trim();
                        let scale = mult[1].trim();
                        let c_idx = clean_gimple_token(idx, temp_map, temp_count, pointer_map);
                        let c_scale = clean_gimple_token(scale, temp_map, temp_count, pointer_map);

                        *temp_count += 1;
                        let t_sc = format!("t{}", *temp_count);
                        tac_lines.push(format!("{t_sc} = {c_idx} * {c_scale}"));

                        *temp_count += 1;
                        let t_ad = format!("t{}", *temp_count);
                        tac_lines.push(format!("{t_ad} = {c_base} + {t_sc}"));

                        return format!("mem_{t_ad}");
                    } else {
                        let offset = rest.trim_end_matches('B').trim();
                        let c_off = clean_gimple_token(offset, temp_map, temp_count, pointer_map);

                        *temp_count += 1;
                        let t_ad = format!("t{}", *temp_count);
                        tac_lines.push(format!("{t_ad} = {c_base} + {c_off}"));

                        return format!("mem_{t_ad}");
                    }
                } else {
                    let ptr = inner.trim_start_matches('&').trim();
                    let c_ptr = clean_gimple_token(ptr, temp_map, temp_count, pointer_map);
                    return format!("mem_{c_ptr}");
                }
            }
        }
    }

    let cleaned = clean_gimple_token(s, temp_map, temp_count, pointer_map);
    format!("mem_{cleaned}")
}

fn parse_gimple_to_tac(gimple_source: &str) -> Result<String, String> {
    let mut temp_map: HashMap<String, String> = HashMap::new();
    let mut pointer_map: HashMap<String, String> = HashMap::new();
    let mut temp_count = 0;
    let mut defined_labels: HashSet<String> = HashSet::new();
    let mut referenced_labels: HashSet<String> = HashSet::new();

    let mut blocks: Vec<GimpleRawBlock> = Vec::new();
    let mut current_block: Option<GimpleRawBlock> = None;
    let mut in_main = false;
    let mut array_declarations: Vec<String> = Vec::new();
    let lines: Vec<&str> = gimple_source.lines().map(str::trim).collect();
    let mut i = 0;

    while i < lines.len() {
        let mut line = lines[i].to_string();
        i += 1;

        if line.contains("main ()") || line.contains("main (") {
            in_main = true;
            continue;
        }

        // If another function starts after main, stop
        if in_main
            && (line.starts_with(";; Function")
                || (line.ends_with(')')
                    && line.contains('(')
                    && !line.starts_with("if")
                    && !line.starts_with("scanf")
                    && !line.starts_with("printf")))
        {
            if !line.contains("main") {
                break;
            }
        }

        if !in_main {
            continue;
        }

        if line.starts_with("int ") && line.contains('[') && line.contains(']') {
            let after = line.trim_start_matches("int ").trim();
            for part in after.split(',') {
                let part = part.trim();
                if let Some(open) = part.find('[') {
                    let name = part[..open].trim();
                    if !name.is_empty() && !array_declarations.contains(&name.to_string()) {
                        array_declarations.push(name.to_string());
                    }
                }
            }
            continue;
        }

        // Basic block label: <bb 2> [local count: ...]: or <bb 3>: or <D.3486>: or label:
        if (line.starts_with("<bb ")
            || line.starts_with("<D.")
            || line.starts_with("bb_")
            || line.ends_with(':'))
            && line.contains(':')
        {
            if let Some(b) = current_block.take() {
                blocks.push(b);
            }
            let lbl = clean_gimple_label(&line);
            current_block = Some(GimpleRawBlock {
                label: lbl,
                phis: Vec::new(),
                stmts: Vec::new(),
            });
            continue;
        }

        let Some(ref mut cur) = current_block else {
            continue;
        };

        // PHI node: # dest = PHI <val1(pred1), val2(pred2), ...>
        if line.starts_with("# ") && line.contains("= PHI <") {
            while !line.contains('>') && i < lines.len() {
                line.push(' ');
                line.push_str(lines[i]);
                i += 1;
            }
            if let (Some(eq_pos), Some(open_b), Some(close_b)) =
                (line.find("= PHI <"), line.find('<'), line.rfind('>'))
            {
                let raw_lhs = line[2..eq_pos].trim();
                let clean_lhs =
                    clean_gimple_token(raw_lhs, &mut temp_map, &mut temp_count, &pointer_map);
                let args_str = &line[open_b + 1..close_b];
                for entry in args_str.split(',') {
                    let entry = entry.trim();
                    if let (Some(op), Some(cp)) = (entry.find('('), entry.rfind(')')) {
                        if cp > op {
                            let val_str = entry[..op].trim();
                            let pred_str = entry[op + 1..cp].trim();
                            let clean_val = clean_gimple_token(
                                val_str,
                                &mut temp_map,
                                &mut temp_count,
                                &pointer_map,
                            );
                            let clean_pred = clean_gimple_label(pred_str);
                            cur.phis.push((clean_lhs.clone(), clean_val, clean_pred));
                        }
                    }
                }
            }
            continue;
        }

        // Skip boilerplate GIMPLE
        if line == "{"
            || line == "}"
            || line == "try"
            || line == "finally"
            || line == "else"
            || line.starts_with("int ")
            || line.starts_with("unsigned ")
            || line.starts_with("sizetype ")
            || line.starts_with("long unsigned ")
            || line.starts_with("void ")
            || line.starts_with("# ")
            || line.contains("CLOBBER")
            || line.is_empty()
        {
            continue;
        }

        cur.stmts.push(line);
    }

    if let Some(b) = current_block {
        blocks.push(b);
    }

    if blocks.is_empty() {
        return Err("No scalar TAC statements could be extracted from GCC intermediate dump".to_string());
    }

    // Pass 1.5: Register edge assignments (pred_label, target_label) -> [(dest, val)]
    let mut edge_assignments: HashMap<(String, String), Vec<(String, String)>> = HashMap::new();
    for block in &blocks {
        for (dest, val, pred) in &block.phis {
            edge_assignments
                .entry((pred.clone(), block.label.clone()))
                .or_default()
                .push((dest.clone(), val.clone()));
        }
    }

    // Pass 2: Lower blocks into TAC
    let mut tac_lines = Vec::new();

    for b_idx in 0..blocks.len() {
        let block = &blocks[b_idx];
        defined_labels.insert(block.label.clone());
        tac_lines.push(format!("{}:", block.label));

        if b_idx == 0 {
            for (idx, arr) in array_declarations.iter().enumerate() {
                let base = (idx + 1) * 10000;
                defined_labels.insert(format!("init_{arr}"));
                tac_lines.push(format!("{arr} = {base}"));
            }
        }

        let mut has_terminator = false;
        let mut s_idx = 0;
        while s_idx < block.stmts.len() {
            let line = &block.stmts[s_idx];
            s_idx += 1;

            if line.starts_with("return ") || line == "return;" || line == "return" {
                has_terminator = true;
                referenced_labels.insert("bb_exit".to_string());
                tac_lines.push("goto bb_exit".to_string());
                break;
            }

            // scanf (...)
            if line.starts_with("scanf") {
                let open = line.find('(').unwrap_or(0);
                let close = line.rfind(')').unwrap_or(line.len());
                let inner = &line[open + 1..close];
                let parts = split_arguments(inner);
                for arg in parts.iter().skip(1) {
                    let trimmed = arg.trim();
                    if trimmed.starts_with('&') {
                        let inner_arg = trimmed.trim_start_matches('&').trim();
                        if inner_arg.contains('[') && inner_arg.ends_with(']') {
                            let mem_loc = lower_mem_expr(inner_arg, &mut temp_map, &mut temp_count, &pointer_map, &mut tac_lines);
                            tac_lines.push(format!("read {mem_loc}"));
                        } else {
                            let var = clean_gimple_token(inner_arg, &mut temp_map, &mut temp_count, &pointer_map);
                            if !var.is_empty() {
                                tac_lines.push(format!("read {var}"));
                            }
                        }
                    } else {
                        let var = clean_gimple_token(trimmed, &mut temp_map, &mut temp_count, &pointer_map);
                        if !var.is_empty() {
                            tac_lines.push(format!("read mem_{var}"));
                        }
                    }
                }
                continue;
            }

            // printf (...)
            if line.starts_with("printf") {
                let open = line.find('(').unwrap_or(0);
                let close = line.rfind(')').unwrap_or(line.len());
                let inner = &line[open + 1..close];
                let parts = split_arguments(inner);
                for arg in parts.iter().skip(1) {
                    let clean_val = clean_gimple_token(arg, &mut temp_map, &mut temp_count, &pointer_map);
                    if !clean_val.is_empty() {
                        tac_lines.push(format!("print {clean_val}"));
                    }
                }
                continue;
            }

            // puts / __builtin_puts ("1")
            if line.starts_with("__builtin_puts") || line.starts_with("puts") {
                if let (Some(q1), Some(q2)) = (line.find('"'), line.rfind('"')) {
                    if q2 > q1 {
                        let text = &line[q1 + 1..q2];
                        if let Ok(num) = text.parse::<i32>() {
                            tac_lines.push(format!("print {num}"));
                        }
                    }
                }
                continue;
            }

            // if (cond)
            if line.starts_with("if (") || line.starts_with("if(") {
                has_terminator = true;
                let open = line.find('(').unwrap_or(0);
                let close = line.rfind(')').unwrap_or(line.len());
                let cond_expr = &line[open + 1..close];

                let mut then_target = None;
                let mut else_target = None;

                if line.contains("goto") {
                    let after = &line[close + 1..];
                    if let Some(else_pos) = after.find("else") {
                        then_target = extract_goto_target(&after[..else_pos]);
                        else_target = extract_goto_target(&after[else_pos + 4..]);
                    } else {
                        then_target = extract_goto_target(after);
                    }
                }

                while (then_target.is_none() || else_target.is_none()) && s_idx < block.stmts.len() {
                    let next_line = &block.stmts[s_idx];
                    if next_line.starts_with("goto ") || next_line.starts_with("goto<") {
                        if then_target.is_none() {
                            then_target = extract_goto_target(next_line);
                            s_idx += 1;
                        } else if else_target.is_none() {
                            else_target = extract_goto_target(next_line);
                            s_idx += 1;
                            break;
                        }
                    } else if next_line == "else" {
                        s_idx += 1;
                    } else {
                        break;
                    }
                }

                if else_target.is_none() && b_idx + 1 < blocks.len() {
                    else_target = Some(blocks[b_idx + 1].label.clone());
                }

                let then_lbl = then_target.unwrap_or_else(|| {
                    if b_idx + 1 < blocks.len() {
                        blocks[b_idx + 1].label.clone()
                    } else {
                        "bb_exit".to_string()
                    }
                });
                let else_lbl = else_target.unwrap_or_else(|| "bb_exit".to_string());

                let (op, left, right) = parse_relational_expr(cond_expr);
                let clean_left = clean_gimple_token(&left, &mut temp_map, &mut temp_count, &pointer_map);
                let clean_right = clean_gimple_token(&right, &mut temp_map, &mut temp_count, &pointer_map);

                emit_relational_branch(
                    op,
                    &clean_left,
                    &clean_right,
                    &block.label,
                    &then_lbl,
                    &else_lbl,
                    &edge_assignments,
                    &mut temp_count,
                    &mut tac_lines,
                    &mut defined_labels,
                    &mut referenced_labels,
                );
                continue;
            }

            // goto target
            if let Some(target) = extract_goto_target(line) {
                has_terminator = true;
                if let Some(edges) = edge_assignments.get(&(block.label.clone(), target.clone())) {
                    for (d, v) in edges {
                        tac_lines.push(format!("{d} = {v}"));
                    }
                }
                referenced_labels.insert(target.clone());
                tac_lines.push(format!("goto {target}"));
                continue;
            }

            // Assignment lhs = rhs
            if let Some(eq_pos) = line.find('=') {
                let lhs = line[..eq_pos].trim();
                let rhs = line[eq_pos + 1..].trim().trim_end_matches(';');

                // Skip compiler return variables like D.3436 = 0 or _8 = 0 at the end of main
                if (lhs.starts_with("D.") || lhs.starts_with('_')) && rhs == "0" && !tac_lines.is_empty() {
                    continue;
                }

                // Strip type casts from rhs
                let clean_rhs_str = if let Some(close_paren) = rhs.find(')') {
                    if rhs.starts_with('(') && !rhs.contains(" * ") && !rhs.contains(" + ") && !rhs.contains(" - ") {
                        rhs[close_paren + 1..].trim()
                    } else {
                        rhs
                    }
                } else {
                    rhs
                };

                // Track pointer address-of: p = &x or p = (type)&x
                if clean_rhs_str.starts_with('&') {
                    let clean_lhs = clean_gimple_token(lhs, &mut temp_map, &mut temp_count, &pointer_map);
                    let pointee = clean_gimple_token(clean_rhs_str.trim_start_matches('&'), &mut temp_map, &mut temp_count, &pointer_map);
                    pointer_map.insert(clean_lhs.clone(), pointee.clone());
                    if !defined_labels.contains(&format!("init_{pointee}")) {
                        defined_labels.insert(format!("init_{pointee}"));
                        tac_lines.push(format!("{pointee} = 1000"));
                    }
                    tac_lines.push(format!("{clean_lhs} = {pointee}"));
                    continue;
                }

                // If LHS is MEM or array indexing: e.g. MEM[...] = rhs or a[j] = rhs
                if lhs.starts_with("MEM") || (lhs.contains('[') && lhs.ends_with(']')) {
                    let mem_dest = lower_mem_expr(lhs, &mut temp_map, &mut temp_count, &pointer_map, &mut tac_lines);
                    let clean_rhs = clean_gimple_token(clean_rhs_str, &mut temp_map, &mut temp_count, &pointer_map);
                    tac_lines.push(format!("{mem_dest} = {clean_rhs}"));
                    continue;
                }

                let clean_lhs = clean_gimple_token(lhs, &mut temp_map, &mut temp_count, &pointer_map);

                // If RHS is MEM or array indexing: e.g. lhs = MEM[...] or lhs = a[j]
                if clean_rhs_str.starts_with("MEM") || (clean_rhs_str.contains('[') && clean_rhs_str.ends_with(']')) {
                    let mem_src = lower_mem_expr(clean_rhs_str, &mut temp_map, &mut temp_count, &pointer_map, &mut tac_lines);
                    tac_lines.push(format!("{clean_lhs} = {mem_src}"));
                    continue;
                }

                // Check for relational expressions on RHS
                let (rel_op, rel_l, rel_r) = parse_relational_expr(clean_rhs_str);
                if let Some(op) = rel_op {
                    let cl_l = clean_gimple_token(&rel_l, &mut temp_map, &mut temp_count, &pointer_map);
                    let cl_r = clean_gimple_token(&rel_r, &mut temp_map, &mut temp_count, &pointer_map);
                    emit_relational_assignment(
                        &clean_lhs,
                        op,
                        &cl_l,
                        &cl_r,
                        &mut temp_count,
                        &mut tac_lines,
                    );
                    continue;
                }

                let tokens: Vec<&str> = clean_rhs_str.split_whitespace().collect();
                if tokens.len() == 3 {
                    let left = clean_gimple_token(tokens[0], &mut temp_map, &mut temp_count, &pointer_map);
                    let op = tokens[1];
                    let right = clean_gimple_token(tokens[2], &mut temp_map, &mut temp_count, &pointer_map);

                    if OpCode::from_binary_token(op).is_some() {
                        tac_lines.push(format!("{clean_lhs} = {left} {op} {right}"));
                    }
                } else if tokens.len() == 1 {
                    let val = clean_gimple_token(tokens[0], &mut temp_map, &mut temp_count, &pointer_map);
                    tac_lines.push(format!("{clean_lhs} = {val}"));
                }
                continue;
            }
        }

        // Fallthrough to next basic block
        if !has_terminator && b_idx + 1 < blocks.len() {
            let next_label = &blocks[b_idx + 1].label;
            if let Some(edges) = edge_assignments.get(&(block.label.clone(), next_label.clone())) {
                for (d, v) in edges {
                    tac_lines.push(format!("{d} = {v}"));
                }
            }
        }
    }

    if tac_lines.is_empty() {
        return Err("No scalar TAC statements could be extracted from GCC intermediate dump".to_string());
    }

    defined_labels.insert("bb_exit".to_string());
    tac_lines.push("bb_exit:".to_string());

    // Ensure any referenced jump target label is defined at the end of the program
    for lbl in &referenced_labels {
        if !defined_labels.contains(lbl) {
            tac_lines.push(format!("{lbl}:"));
        }
    }

    Ok(tac_lines.join("\n"))
}

/// Sanitizes a TAC buffer that may contain raw GIMPLE / GCC dump artifacts
/// (such as branch probabilities `bb_3>;_[89_00%]`, `MEM_(...)`, or raw `if (...)`).
pub fn sanitize_gimple_tac(source: &str) -> String {
    let mut tac_lines = Vec::new();
    let mut defined_labels: HashSet<String> = HashSet::new();
    let mut referenced_labels: HashSet<String> = HashSet::new();
    let mut temp_counter = 100;

    let lines: Vec<&str> = source.lines().map(str::trim).collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        i += 1;

        if line.is_empty() {
            continue;
        }

        // Check if line is a label definition
        if line.ends_with(':') {
            let label = clean_gimple_label(line.trim_end_matches(':'));
            defined_labels.insert(label.clone());
            tac_lines.push(format!("{label}:"));
            continue;
        }

        // Check for raw 'if (cond)' followed by gotos
        if line.starts_with("if (") || line.starts_with("if(") {
            let open = line.find('(').unwrap_or(0);
            let close = line.rfind(')').unwrap_or(line.len());
            let cond_expr = &line[open + 1..close];

            let mut then_target = None;
            let mut else_target = None;

            if line.contains("goto") {
                let after = &line[close + 1..];
                if let Some(else_pos) = after.find("else") {
                    then_target = extract_goto_target(&after[..else_pos]);
                    else_target = extract_goto_target(&after[else_pos + 4..]);
                } else {
                    then_target = extract_goto_target(after);
                }
            }

            while (then_target.is_none() || else_target.is_none()) && i < lines.len() {
                let next_line = lines[i];
                if next_line.starts_with("goto") {
                    if then_target.is_none() {
                        then_target = extract_goto_target(next_line);
                        i += 1;
                    } else if else_target.is_none() {
                        else_target = extract_goto_target(next_line);
                        i += 1;
                        break;
                    }
                } else if next_line == "else" {
                    i += 1;
                } else {
                    break;
                }
            }

            let (op, left, right) = parse_relational_expr(cond_expr);
            let then_lbl = then_target.unwrap_or_else(|| "bb_target".to_string());
            let else_lbl = else_target.unwrap_or_else(|| "bb_target".to_string());

            temp_counter += 1;
            let t_diff = format!("t_diff_{temp_counter}");
            temp_counter += 1;
            let t_sign = format!("t_sign_{temp_counter}");
            temp_counter += 1;
            let t_cond = format!("t_cond_{temp_counter}");

            match op {
                Some(RelOp::Eq) => {
                    tac_lines.push(format!("{t_diff} = {left} - {right}"));
                    referenced_labels.insert(then_lbl.clone());
                    referenced_labels.insert(else_lbl.clone());
                    tac_lines.push(format!("if_false {t_diff} goto {then_lbl}"));
                    tac_lines.push(format!("goto {else_lbl}"));
                }
                _ => {
                    match op {
                        Some(RelOp::Lt) => {
                            tac_lines.push(format!("{t_diff} = {left} - {right}"));
                            tac_lines.push(format!("{t_sign} = {t_diff} >> 31"));
                            tac_lines.push(format!("{t_cond} = 0 - {t_sign}"));
                        }
                        Some(RelOp::Gt) => {
                            tac_lines.push(format!("{t_diff} = {right} - {left}"));
                            tac_lines.push(format!("{t_sign} = {t_diff} >> 31"));
                            tac_lines.push(format!("{t_cond} = 0 - {t_sign}"));
                        }
                        Some(RelOp::Le) => {
                            tac_lines.push(format!("{t_diff} = {right} - {left}"));
                            tac_lines.push(format!("{t_sign} = {t_diff} >> 31"));
                            temp_counter += 1;
                            let t_gt = format!("t_gt_{temp_counter}");
                            tac_lines.push(format!("{t_gt} = 0 - {t_sign}"));
                            tac_lines.push(format!("{t_cond} = 1 - {t_gt}"));
                        }
                        Some(RelOp::Ge) => {
                            tac_lines.push(format!("{t_diff} = {left} - {right}"));
                            tac_lines.push(format!("{t_sign} = {t_diff} >> 31"));
                            temp_counter += 1;
                            let t_lt = format!("t_lt_{temp_counter}");
                            tac_lines.push(format!("{t_lt} = 0 - {t_sign}"));
                            tac_lines.push(format!("{t_cond} = 1 - {t_lt}"));
                        }
                        Some(RelOp::Ne) => {
                            tac_lines.push(format!("{t_cond} = {left} - {right}"));
                        }
                        Some(RelOp::Eq) => unreachable!("handled in outer match"),
                        None => {
                            tac_lines.push(format!("{t_cond} = {left}"));
                        }
                    }
                    referenced_labels.insert(else_lbl.clone());
                    referenced_labels.insert(then_lbl.clone());
                    tac_lines.push(format!("if_false {t_cond} goto {else_lbl}"));
                    tac_lines.push(format!("goto {then_lbl}"));
                }
            }
            continue;
        }

        // Clean gotos: goto bb_3>;_[89_00%] -> goto bb_3
        if line.starts_with("goto") {
            if let Some(target) = extract_goto_target(line) {
                referenced_labels.insert(target.clone());
                tac_lines.push(format!("goto {target}"));
            }
            continue;
        }

        // Clean if_false destination: if_false x goto bb_3>;_[89_00%]
        if line.starts_with("if_false") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let cond = parts[1];
                let target = clean_gimple_label(parts[3]);
                referenced_labels.insert(target.clone());
                tac_lines.push(format!("if_false {cond} goto {target}"));
                continue;
            }
        }

        // Clean MEM assignments: MEM_(int *)_121 = temp_82 -> mem_121 = temp_82
        if line.contains("MEM") {
            let mut cleaned_line = line
                .replace("(int *)", "")
                .replace("(void *)", "")
                .replace("(int)", "")
                .replace(['(', ')', '[', ']', '*'], "")
                .replace("MEM_", "mem_")
                .replace("MEM", "mem_");
            while cleaned_line.contains("__") {
                cleaned_line = cleaned_line.replace("__", "_");
            }
            let tokens: Vec<&str> = cleaned_line.split_whitespace().collect();
            tac_lines.push(tokens.join(" "));
            continue;
        }

        tac_lines.push(line.to_string());
    }

    // Emit any missing referenced labels at the end so CFG is complete
    for lbl in &referenced_labels {
        if !defined_labels.contains(lbl) {
            tac_lines.push(format!("{lbl}:"));
        }
    }

    tac_lines.join("\n")
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::build;
    use crate::parser::parse_program;
    use crate::passes::optimize_with_worklist;
    use crate::vm::execute;

    #[test]
    fn test_quicksort_lowering_and_execution() {
        let qs_code = r#"
#include <stdio.h>

int main() {
    int n;
    scanf("%d", &n);
    int a[100];
    for (int i = 0; i < n; i++) {
        scanf("%d", &a[i]);
    }

    int low[100], high[100];
    int top = -1;
    top++;
    low[top] = 0;
    high[top] = n - 1;

    while (top >= 0) {
        int l = low[top];
        int h = high[top];
        top--;

        int pivot = a[h];
        int i = l - 1;
        for (int j = l; j <= h - 1; j++) {
            if (a[j] <= pivot) {
                i++;
                int temp = a[i];
                a[i] = a[j];
                a[j] = temp;
            }
        }
        int temp = a[i + 1];
        a[i + 1] = a[h];
        a[h] = temp;
        int p = i + 1;

        if (p - 1 > l) {
            top++;
            low[top] = l;
            high[top] = p - 1;
        }
        if (p + 1 < h) {
            top++;
            low[top] = p + 1;
            high[top] = h;
        }
    }

    for (int i = 0; i < n; i++) {
        printf("%d\n", a[i]);
    }
    return 0;
}
"#;
        let tac = lower_c_source_via_gcc_backup(qs_code).expect("Should lower quicksort to TAC");
        let mut parsed = parse_program(&tac).expect("Generated TAC should parse cleanly");
        let res = crate::vm::execute_permissive(&parsed, &[4, 9, 3, 7, 5], 10000)
            .expect("VM should execute without infinite loop");
        println!("Quicksort TAC:\n{}", tac);
        println!("Quicksort res.output: {:?}", res.output);
        assert!(res.steps < 2000, "Should finish in well under 2000 steps, took {}", res.steps);
        assert_eq!(res.output, vec![3, 5, 7, 9], "Quicksort must output sorted array");

        let cfg = crate::cfg::build(&mut parsed).expect("CFG build");
        let _report = crate::passes::optimize_with_worklist(&mut parsed, &cfg);
        let opt_res = crate::vm::execute_permissive(&parsed, &[4, 9, 3, 7, 5], 10000)
            .expect("Optimized quicksort VM run");
        assert_eq!(opt_res.output, vec![3, 5, 7, 9], "Optimized quicksort must match original sorted output");
    }

    #[test]
    fn test_bubble_sort_lowering_and_execution() {
        let bubble_code = r#"
#include <stdio.h>
int main() {
    int n;
    scanf("%d", &n);
    int a[100];
    for (int i = 0; i < n; i++) {
        scanf("%d", &a[i]);
    }
    for (int i = 0; i < n - 1; i++) {
        for (int j = 0; j < n - i - 1; j++) {
            if (a[j] > a[j + 1]) {
                int temp = a[j];
                a[j] = a[j + 1];
                a[j + 1] = temp;
            }
        }
    }
    for (int i = 0; i < n; i++) {
        printf("%d\n", a[i]);
    }
    return 0;
}
"#;
        let tac = lower_c_source_via_gcc_backup(bubble_code).expect("Should lower bubble sort to TAC");
        println!("Bubble sort TAC:\n{}", tac);
        let mut parsed = parse_program(&tac).expect("Generated TAC should parse cleanly");
        let res = crate::vm::execute_permissive(&parsed, &[4, 7, 8, 6, 5], 100000)
            .expect("VM should execute without infinite loop");
        println!("Bubble sort res.output: {:?}", res.output);
        assert_eq!(res.output, vec![5, 6, 7, 8]);

        // Optimize
        let cfg = crate::cfg::build(&mut parsed).expect("CFG build");
        let report = crate::passes::optimize_with_worklist(&mut parsed, &cfg);
        println!("Transformations count: {}", report.transformations.len());
        for t in &report.transformations {
            println!("{:?}: {} -> {}", t.pass, t.before, t.after);
        }
        println!("Optimized TAC:\n{}", parsed);
        let opt_res = crate::vm::execute_permissive(&parsed, &[4, 7, 8, 6, 5], 100000)
            .expect("Optimized VM run");
        println!("Optimized Bubble sort res.output: {:?}", opt_res.output);
        assert_eq!(opt_res.output, vec![5, 6, 7, 8]);
    }

    #[test]
    fn test_relational_edge_cases_in_loops() {
        // 1. Countdown while loop (top >= 0) with negative termination
        let countdown_code = r#"
#include <stdio.h>
int main() {
    int top = 5;
    int sum = 0;
    while (top >= 0) {
        sum += top;
        top--;
    }
    printf("%d\n", sum);
    return 0;
}
"#;
        let tac1 = lower_c_source_via_gcc_backup(countdown_code).expect("Lower countdown");
        let prog1 = parse_program(&tac1).expect("Parse countdown TAC");
        let res1 = crate::vm::execute_permissive(&prog1, &[], 5000).expect("Run countdown");
        let gcc_out1 = run_c_via_gcc(countdown_code, "").expect("GCC native run");
        assert_eq!(res1.output, gcc_out1, "Countdown output must match native GCC");
        assert_eq!(res1.output, vec![15]);

        // 2. Nested loop with <= condition and break/continue
        let nested_code = r#"
#include <stdio.h>
int main() {
    int sum = 0;
    for (int i = 0; i <= 3; i++) {
        for (int j = 0; j <= 2; j++) {
            if (j == 1) continue;
            sum += i * 10 + j;
        }
    }
    printf("%d\n", sum);
    return 0;
}
"#;
        let tac2 = lower_c_source_via_gcc_backup(nested_code).expect("Lower nested");
        let prog2 = parse_program(&tac2).expect("Parse nested TAC");
        let res2 = crate::vm::execute_permissive(&prog2, &[], 5000).expect("Run nested");
        let gcc_out2 = run_c_via_gcc(nested_code, "").expect("GCC native run");
        assert_eq!(res2.output, gcc_out2, "Nested output must match native GCC");

        // 3. Negative comparison edge cases
        let neg_code = r#"
#include <stdio.h>
int main() {
    int a = -10, b = -5;
    if (a < b) printf("1\n"); else printf("0\n");
    if (a > b) printf("1\n"); else printf("0\n");
    if (a <= -10) printf("1\n"); else printf("0\n");
    if (b >= -5) printf("1\n"); else printf("0\n");
    if (a == -10) printf("1\n"); else printf("0\n");
    if (a != b) printf("1\n"); else printf("0\n");
    return 0;
}
"#;
        let tac3 = lower_c_source_via_gcc_backup(neg_code).expect("Lower negative comp");
        let prog3 = parse_program(&tac3).expect("Parse neg TAC");
        let res3 = crate::vm::execute_permissive(&prog3, &[], 5000).expect("Run neg");
        let gcc_out3 = run_c_via_gcc(neg_code, "").expect("GCC native run");
        assert_eq!(res3.output, gcc_out3, "Negative comparison must match native GCC");
        assert_eq!(res3.output, vec![1, 0, 1, 1, 1, 1]);
    }

    #[test]
    fn test_looks_like_c_code() {
        assert!(looks_like_c_code("#include <stdio.h>\nint main() { return 0; }"));
        assert!(looks_like_c_code("int a = 10;"));
        assert!(looks_like_c_code("scanf(\"%d\", &a);"));
        assert!(!looks_like_c_code("read a\nt1 = a + 0\nprint t1"));
    }

    #[test]
    fn test_lower_c_simple_math() {
        let c_code = r#"
            #include <stdio.h>
            int main() {
                int a, b;
                scanf("%d %d", &a, &b);
                int c = a + b;
                int d = c * 2;
                printf("%d\n", d);
                return 0;
            }
        "#;

        let tac = lower_c_to_tac(c_code).expect("Lowering should succeed");
        assert!(tac.contains("read a"));
        assert!(tac.contains("read b"));
        assert!(tac.contains("c = a + b"));
        assert!(tac.contains("d = c * 2"));
        assert!(tac.contains("print d"));

        let mut prog = parse_program(&tac).expect("TAC should parse");
        let cfg = build(&mut prog).expect("CFG should build");
        let report = optimize_with_worklist(&mut prog, &cfg);

        // Verification on VM
        let res = execute(&prog, &[5, 10]).expect("VM execution");
        assert_eq!(res.output, vec![30]);
        assert!(!report.transformations.is_empty());
    }

    #[test]
    fn test_lower_c_compound_expression_precedence() {
        let c_code = r#"
            int main() {
                int a = 10;
                int b = 20;
                int res = (a + b) * 2 + 5;
                printf("%d\n", res);
            }
        "#;

        let tac = lower_c_to_tac(c_code).expect("Lowering should succeed");
        let mut prog = parse_program(&tac).expect("TAC should parse");
        let cfg = build(&mut prog).expect("CFG should build");
        let _ = optimize_with_worklist(&mut prog, &cfg);

        let res = execute(&prog, &[]).expect("VM execution");
        // (10 + 20) * 2 + 5 = 65
        assert_eq!(res.output, vec![65]);
    }

    #[test]
    fn test_lower_c_strength_reduction() {
        let c_code = r#"
            #include <stdio.h>
            int main() {
                int x;
                scanf("%d", &x);
                int m = x * 8;
                int d = x / 4;
                printf("%d\n", m);
                printf("%d\n", d);
                return 0;
            }
        "#;

        let tac = lower_c_to_tac(c_code).expect("Lowering should succeed");
        let mut prog = parse_program(&tac).expect("TAC should parse");
        let cfg = build(&mut prog).expect("CFG should build");
        let report = optimize_with_worklist(&mut prog, &cfg);

        // Verify strength reduction triggered
        let sr_count = report
            .transformations
            .iter()
            .filter(|t| t.pass == crate::passes::OptimizationKind::StrengthReduction)
            .count();
        assert!(sr_count >= 2, "Expected at least 2 strength reductions, got {sr_count}");

        let res = execute(&prog, &[16]).expect("VM execution");
        // 16 * 8 = 128, 16 / 4 = 4
        assert_eq!(res.output, vec![128, 4]);
    }

    #[test]
    fn test_lower_c_control_flow() {
        let c_code = r#"
            int main() {
                int a;
                scanf("%d", &a);
                if (a == 0) goto zero_label;
                int b = a * 2;
                printf("%d\n", b);
                goto end_label;
            zero_label:
                printf("%d\n", 0);
            end_label:
                return 0;
            }
        "#;

        let tac = lower_c_to_tac(c_code).expect("Lowering should succeed");
        assert!(tac.contains("if_false a goto zero_label"));
        assert!(tac.contains("goto end_label"));
        assert!(tac.contains("zero_label:"));

        let mut prog = parse_program(&tac).expect("TAC should parse");
        let cfg = build(&mut prog).expect("CFG should build");
        let _ = optimize_with_worklist(&mut prog, &cfg);

        let res_zero = execute(&prog, &[0]).expect("VM execution for 0");
        assert_eq!(res_zero.output, vec![0]);

        let res_five = execute(&prog, &[5]).expect("VM execution for 5");
        assert_eq!(res_five.output, vec![10]);
    }

    #[test]
    fn test_gimple_dump_sanitization_and_optimization() {
        let raw_gimple_tac = r#"
read n
t1 = n
t2 = t1
t3 = t2 * 4
goto bb_3>;_[89_00%]
goto bb_23>;_[11_00%]
t4 = a_56
t5 = t6
read t5
i_87 = i_100 + 1
t7 = n
t8 = t6 + 4
goto bb_4>;_[89_00%]
goto bb_5>;_[11_00%]
goto bb_6>;_[66_29%]
goto bb_23>;_[33_71%]
low_58 = 0
t9 = t7 + -1
high_60 = t9
goto bb_16>;_[100_00%]
t10 = top_46
t11 = t10 * 4
t12 = low_58 + t11
l_69 = t12
top_70 = top_46 + -1
t13 = high_60 + t11
h_71 = t13
t14 = h_71
t15 = t14 * 4
t16 = a_56 + t15
i_73 = l_69 + -1
goto bb_8>;_[89_00%]
goto bb_22>;_[11_00%]
pivot_72 = t16
t17 = l_69
t18 = t17 * 4
t19 = a_56
t20 = t18 + t19
t21 = h_71
t22 = l_69
t23 = t21 - t22
t24 = t23
t25 = t17 + t24
t26 = t25 * 4
t27 = t19 + t26
t28 = t29
if (_21 < = pivot_72)
goto bb_10>;_[50_00%]
goto bb_11>;_[50_00%]
i_81 = i_101 + 1
t31 = i_81
t32 = t31 * 4
t33 = a_56 + t32
temp_82 = t33
t33 = t30
MEM_(int *)_121 = temp_82
t34 = t29 + 4
t35 = t36
goto bb_9>;_[89_00%]
goto bb_12>;_[11_00%]
t37 = i_41
t38 = t37 + 1
t39 = t38 * 4
t40 = a_56 + t39
temp_74 = t40
t41 = t16
t40 = t41
t16 = temp_74
goto bb_13>;_[56_18%]
goto bb_14>;_[43_82%]
t13 = i_41
t42 = i_93 + 2
goto bb_15>;_[5_50%]
goto bb_18>;_[94_50%]
top_78 = top_45 + 1
t43 = top_78
t44 = t43 * 4
t45 = low_58 + t44
t45 = t42
t46 = high_60 + t44
t46 = h_71
goto bb_17>;_[100_00%]
if (top_46 > = 0)
goto bb_7>;_[94_50%]
goto bb_19>;_[5_50%]
t47 = n
goto bb_20>;_[89_00%]
goto bb_21>;_[11_00%]
print t48
t49 = n
t50 = t51 + 1
i_114 = t50
goto bb_20>;_[89_00%]
goto bb_21>;_[11_00%]
t52 = l_69
t53 = t52 * 4
t54 = a_56 + t53
temp_47 = t54
t55 = t16
t54 = t55
t16 = temp_47
goto bb_14>;_[100_00%]
        "#;

        let clean_tac = sanitize_gimple_tac(raw_gimple_tac);

        // Verify artifact removal
        assert!(!clean_tac.contains(">;_["));
        assert!(!clean_tac.contains("%"));
        assert!(!clean_tac.contains("MEM_"));
        assert!(!clean_tac.contains("(int *)"));
        assert!(clean_tac.contains("goto bb_3"));
        assert!(clean_tac.contains("goto bb_23"));
        assert!(clean_tac.contains("mem_121 = temp_82"));

        // Verify it parses without error
        let mut prog = parse_program(&clean_tac).expect("Cleaned TAC must parse successfully");

        // Verify CFG builds with all labels resolved
        let cfg = build(&mut prog).expect("CFG must build with all jump targets resolved");

        // Verify optimization pipeline executes and fires passes
        let report = optimize_with_worklist(&mut prog, &cfg);
        assert!(!report.transformations.is_empty());

        let sr_fired = report.transformations.iter().any(|t| t.pass == crate::passes::OptimizationKind::StrengthReduction);
        let cp_fired = report.transformations.iter().any(|t| t.pass == crate::passes::OptimizationKind::ConstantPropagation);
        let dce_fired = report.transformations.iter().any(|t| t.pass == crate::passes::OptimizationKind::DeadCodeElimination);

        assert!(sr_fired, "Strength Reduction must fire on * 4 rewrites");
        assert!(cp_fired, "Constant Propagation must fire on alias chains");
        assert!(dce_fired, "Dead Code Elimination must prune dead temporaries");
    }
}

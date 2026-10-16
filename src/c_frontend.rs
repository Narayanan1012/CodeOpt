//! Dual-Tier C-to-TAC Frontend Adapter
//!
//! Provides two tiers of lowering C source code into native TAC format:
//! - Tier 1 (Active Default): Native pure-Rust in-memory C-subset lowerer (zero external dependencies).
//! - Tier 2 (Strictly Dormant Backup): GCC GIMPLE bridge invoked only via explicit CLI `c-backup`.

use std::collections::HashMap;
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

    let temp_dir = std::env::temp_dir().join(format!("codeopt_gimple_{}", std::process::id()));
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
    let temp_dir = std::env::temp_dir().join(format!("codeopt_gimple_src_{}", std::process::id()));
    fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("Could not create temporary directory: {e}"))?;
    let temp_c_path = temp_dir.join("input.c");
    fs::write(&temp_c_path, c_source)
        .map_err(|e| format!("Failed to write temporary C source: {e}"))?;
    let res = lower_c_via_gcc_backup(&temp_c_path.to_string_lossy());
    let _ = fs::remove_dir_all(&temp_dir);
    res
}

fn clean_gimple_token(
    raw: &str,
    temp_map: &mut HashMap<String, String>,
    temp_count: &mut usize,
    pointer_map: &HashMap<String, String>,
) -> String {
    let mut token = raw.trim();
    // Strip type casts like (int), (long unsigned int), (sizetype)
    while token.starts_with('(') {
        if let Some(close_paren) = token.find(')') {
            token = token[close_paren + 1..].trim();
        } else {
            break;
        }
    }
    // Pointer dereference: *a or *_1
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
    // Array indexing: arr[0] -> arr_0
    if token.contains('[') && token.contains(']') {
        let cleaned = token.replace('[', "_").replace(']', "");
        return cleaned;
    }
    // Check if it's already a clean integer constant
    if token.parse::<i32>().is_ok() {
        return token.to_string();
    }
    // Compiler temporaries: _1, x.1_2, D.3440, etc.
    if token.starts_with('_') || token.contains('.') || token.contains('#') {
        if let Some(mapped) = temp_map.get(token) {
            return mapped.clone();
        }
        *temp_count += 1;
        let fresh = format!("t{}", *temp_count);
        temp_map.insert(token.to_string(), fresh.clone());
        return fresh;
    }
    token.to_string()
}

fn parse_gimple_to_tac(gimple_source: &str) -> Result<String, String> {
    let mut tac_lines = Vec::new();
    let mut temp_map: HashMap<String, String> = HashMap::new();
    let mut pointer_map: HashMap<String, String> = HashMap::new();
    let mut temp_count = 0;

    let mut in_main = false;

    for raw_line in gimple_source.lines() {
        let line = raw_line.trim();

        if line.contains("main ()") || line.contains("main (") {
            in_main = true;
            continue;
        }

        // If another function starts after main, stop
        if in_main && (line.starts_with(";; Function") || (line.ends_with(')') && line.contains('(') && !line.starts_with("if") && !line.starts_with("scanf") && !line.starts_with("printf"))) {
            if !line.contains("main") {
                break;
            }
        }

        if !in_main {
            continue;
        }

        // Skip boilerplate GIMPLE
        if line == "{"
            || line == "}"
            || line == "try"
            || line == "finally"
            || line.starts_with("int ")
            || line.starts_with("void ")
            || line.starts_with("return ")
            || line.starts_with("<bb ")
            || line.starts_with("# ")
            || line.starts_with("MEM <")
            || line.contains("CLOBBER")
            || line.is_empty()
        {
            continue;
        }

        // scanf ("...", &a, &b);
        if line.starts_with("scanf") {
            let open = line.find('(').unwrap_or(0);
            let close = line.rfind(')').unwrap_or(line.len());
            let inner = &line[open + 1..close];
            let parts = split_arguments(inner);
            for arg in parts.iter().skip(1) {
                let var = clean_gimple_token(arg, &mut temp_map, &mut temp_count, &pointer_map);
                if !var.is_empty() {
                    tac_lines.push(format!("read {var}"));
                }
            }
            continue;
        }

        // printf ("...", c);
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

        // GIMPLE binary/copy assignment: dest = left op right; or dest = val;
        if let Some(eq_pos) = line.find('=') {
            let lhs = line[..eq_pos].trim();
            let rhs = line[eq_pos + 1..].trim().trim_end_matches(';');

            // Skip compiler return variables like D.3436 = 0 or _8 = 0 at the end of main
            if (lhs.starts_with("D.") || lhs.starts_with('_')) && rhs == "0" && !tac_lines.is_empty() {
                continue;
            }

            let clean_lhs = clean_gimple_token(lhs, &mut temp_map, &mut temp_count, &pointer_map);

            // Track pointer address-of: p = &x
            if rhs.trim().starts_with('&') {
                let pointee = clean_gimple_token(rhs.trim().trim_start_matches('&'), &mut temp_map, &mut temp_count, &pointer_map);
                pointer_map.insert(clean_lhs.clone(), pointee);
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

        // goto <D.xxxx> or goto label
        if let Some(target) = line.strip_prefix("goto ") {
            let label = target
                .trim()
                .trim_end_matches(';')
                .trim_start_matches('<')
                .trim_end_matches('>')
                .replace([' ', '.'], "_");
            tac_lines.push(format!("goto {label}"));
            continue;
        }

        // Label: <D.xxxx>: or <bb 2>: or label:
        if line.ends_with(':') {
            let label = line
                .trim_end_matches(':')
                .trim_start_matches('<')
                .trim_end_matches('>')
                .replace([' ', '.'], "_");
            tac_lines.push(format!("{label}:"));
            continue;
        }
    }

    if tac_lines.is_empty() {
        return Err("No scalar TAC statements could be extracted from GCC intermediate dump".to_string());
    }

    Ok(tac_lines.join("\n"))
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
}

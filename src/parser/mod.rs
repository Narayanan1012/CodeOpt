//! Parser boundary for the native `.tac` format.

use std::fmt;

use crate::ir::{Instruction, OpCode, Operand, Program};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parses the Review 1 TAC grammar.
///
/// Supported forms are `label:`, `goto label`, `if_false value goto label`,
/// `read destination`, `print value`, `destination = value`, and
/// `destination = value operator value`. Empty lines and `#` comments are
/// ignored.
pub fn parse_program(source: &str) -> Result<Program, ParseError> {
    let mut instructions = Vec::new();

    for (line_index, raw_line) in source.lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw_line.split('#').next().unwrap_or_default().trim();

        if line.is_empty() {
            continue;
        }

        let instruction = parse_line(line, instructions.len(), line_number)?;
        instructions.push(instruction);
    }

    Ok(Program::new(instructions))
}

fn parse_line(line: &str, id: usize, original_line: usize) -> Result<Instruction, ParseError> {
    if let Some(label) = line.strip_suffix(':') {
        let label = parse_identifier(label.trim(), original_line, "label")?;
        return Ok(instruction(
            id,
            OpCode::Label,
            Operand::None,
            Operand::None,
            Operand::Label(label),
            original_line,
        ));
    }

    let tokens = line.split_whitespace().collect::<Vec<_>>();
    match tokens.as_slice() {
        ["goto", label] => Ok(instruction(
            id,
            OpCode::Goto,
            Operand::None,
            Operand::None,
            Operand::Label(parse_identifier(label, original_line, "jump label")?),
            original_line,
        )),
        ["if_false", condition, "goto", label] => Ok(instruction(
            id,
            OpCode::IfFalse,
            parse_value(condition, original_line)?,
            Operand::None,
            Operand::Label(parse_identifier(label, original_line, "jump label")?),
            original_line,
        )),
        ["read", destination] => Ok(instruction(
            id,
            OpCode::Read,
            Operand::None,
            Operand::None,
            parse_destination(destination, original_line)?,
            original_line,
        )),
        ["print", value] => Ok(instruction(
            id,
            OpCode::Print,
            parse_value(value, original_line)?,
            Operand::None,
            Operand::None,
            original_line,
        )),
        _ => parse_assignment(&tokens, id, original_line),
    }
}

fn parse_assignment(
    tokens: &[&str],
    id: usize,
    original_line: usize,
) -> Result<Instruction, ParseError> {
    let [destination, "=", rest @ ..] = tokens else {
        return Err(error(original_line, "expected a supported TAC instruction"));
    };

    let destination = parse_destination(destination, original_line)?;
    match rest {
        [value] => Ok(instruction(
            id,
            OpCode::Assign,
            parse_value(value, original_line)?,
            Operand::None,
            destination,
            original_line,
        )),
        [left, operator, right] => {
            let operation = OpCode::from_binary_token(operator).ok_or_else(|| {
                error(
                    original_line,
                    "binary operator must be one of: +, -, *, /, %",
                )
            })?;
            Ok(instruction(
                id,
                operation,
                parse_value(left, original_line)?,
                parse_value(right, original_line)?,
                destination,
                original_line,
            ))
        }
        _ => Err(error(
            original_line,
            "assignment must have one value or a binary expression",
        )),
    }
}

fn parse_value(token: &str, line: usize) -> Result<Operand, ParseError> {
    if let Ok(value) = token.parse::<i32>() {
        return Ok(Operand::Const(value));
    }

    parse_variable(token, line)
}

fn parse_destination(token: &str, line: usize) -> Result<Operand, ParseError> {
    parse_variable(token, line)
}

fn parse_variable(token: &str, line: usize) -> Result<Operand, ParseError> {
    let name = parse_identifier(token, line, "variable")?;
    if let Some(index) = name
        .strip_prefix('t')
        .and_then(|suffix| suffix.parse::<usize>().ok())
    {
        return Ok(Operand::Temp(index));
    }
    Ok(Operand::Var(name))
}

fn parse_identifier(token: &str, line: usize, kind: &str) -> Result<String, ParseError> {
    let mut characters = token.chars();
    let Some(first) = characters.next() else {
        return Err(error(line, format!("{kind} cannot be empty")));
    };

    if !(first.is_ascii_alphabetic() || first == '_')
        || !characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(error(line, format!("invalid {kind} `{token}`")));
    }

    Ok(token.to_owned())
}

fn instruction(
    id: usize,
    op: OpCode,
    arg1: Operand,
    arg2: Operand,
    dest: Operand,
    original_line: usize,
) -> Instruction {
    Instruction {
        id,
        op,
        arg1,
        arg2,
        dest,
        block_id: None,
        original_line,
    }
}

fn error(line: usize, message: impl Into<String>) -> ParseError {
    ParseError {
        line,
        message: message.into(),
    }
}

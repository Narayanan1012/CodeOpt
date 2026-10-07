use std::env;
use std::fs;

use codeopt::cfg::build;
use codeopt::parser::parse_program;
use codeopt::vm::execute;

fn main() {
    let command = env::args().nth(1).unwrap_or_else(|| "status".to_owned());

    match command.as_str() {
        "status" => {
            println!("CodeOpt Milestone 1 initialized");
            println!("Next: parse TAC, construct CFG, execute TAC, then add local passes.");
        }
        "help" | "--help" | "-h" => {
            println!(
                "Usage: codeopt [status | format <file.tac> | cfg <file.tac> | run <file.tac> [inputs]"
            );
        }
        "format" => {
            let Some(path) = env::args().nth(2) else {
                eprintln!("Usage: codeopt format <file.tac>");
                std::process::exit(2);
            };
            let source = fs::read_to_string(&path).unwrap_or_else(|error| {
                eprintln!("Could not read `{path}`: {error}");
                std::process::exit(1);
            });
            let program = parse_program(&source).unwrap_or_else(|error| {
                eprintln!("Invalid TAC: {error}");
                std::process::exit(1);
            });
            println!("{program}");
        }
        "cfg" => {
            let Some(path) = env::args().nth(2) else {
                eprintln!("Usage: codeopt cfg <file.tac>");
                std::process::exit(2);
            };
            let source = fs::read_to_string(&path).unwrap_or_else(|error| {
                eprintln!("Could not read `{path}`: {error}");
                std::process::exit(1);
            });
            let mut program = parse_program(&source).unwrap_or_else(|error| {
                eprintln!("Invalid TAC: {error}");
                std::process::exit(1);
            });
            let cfg = build(&mut program).unwrap_or_else(|error| {
                eprintln!("Invalid control flow: {error}");
                std::process::exit(1);
            });
            for block in cfg.blocks {
                println!(
                    "B{}: instructions {:?}; predecessors {:?}; successors {:?}",
                    block.id, block.instruction_ids, block.predecessors, block.successors
                );
            }
        }
        "run" => {
            let Some(path) = env::args().nth(2) else {
                eprintln!("Usage: codeopt run <file.tac> [inputs]");
                std::process::exit(2);
            };
            let inputs = env::args().nth(3).map_or_else(Vec::new, |raw_inputs| {
                raw_inputs
                    .split(',')
                    .map(|token| {
                        token.trim().parse::<i32>().unwrap_or_else(|_| {
                            eprintln!("Invalid input integer: `{token}`");
                            std::process::exit(2);
                        })
                    })
                    .collect()
            });
            let source = fs::read_to_string(&path).unwrap_or_else(|error| {
                eprintln!("Could not read `{path}`: {error}");
                std::process::exit(1);
            });
            let program = parse_program(&source).unwrap_or_else(|error| {
                eprintln!("Invalid TAC: {error}");
                std::process::exit(1);
            });
            let result = execute(&program, &inputs).unwrap_or_else(|error| {
                eprintln!("Execution failed: {error}");
                std::process::exit(1);
            });
            for value in result.output {
                println!("{value}");
            }
        }
        other => {
            eprintln!("Unknown command: {other}. Run `codeopt help`.");
            std::process::exit(2);
        }
    }
}

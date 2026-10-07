use std::env;
use std::fs;

use codeopt::cfg::build;
use codeopt::parser::parse_program;
use codeopt::passes::optimize_with_worklist;
use codeopt::tui::{build_demo, run_tui};
use codeopt::verification::verify;
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
                "Usage: codeopt [status | format <file.tac> | cfg <file.tac> | run <file.tac> [inputs] | optimize <file.tac> | verify <file.tac> [inputs] | tui [file.tac] [inputs]]"
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
        "optimize" => {
            let Some(path) = env::args().nth(2) else {
                eprintln!("Usage: codeopt optimize <file.tac>");
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
            let report = optimize_with_worklist(&mut program, &cfg);
            for transformation in report.transformations {
                println!(
                    "{:?} [instruction {}]: {} => {}",
                    transformation.pass,
                    transformation.instruction_id,
                    transformation.before,
                    transformation.after
                );
            }
            for event in report.worklist_events {
                println!(
                    "Worklist: instruction {} changed; queued consumer {}",
                    event.definition_id, event.consumer_id
                );
            }
            if report.reached_round_limit {
                println!(
                    "Worklist stopped at the {}-round safety limit.",
                    report.rounds
                );
            } else {
                println!(
                    "Worklist reached a fixed point in {} round(s).",
                    report.rounds
                );
            }
            println!("\nOptimized TAC:\n{program}");
        }
        "tui" => {
            let path = env::args()
                .nth(2)
                .unwrap_or_else(|| "benchmarks/cse_01.tac".to_owned());
            let inputs = env::args()
                .nth(3)
                .map(|raw_inputs| parse_inputs(&raw_inputs))
                .unwrap_or_else(|| {
                    if path == "benchmarks/cse_01.tac" {
                        vec![4, 9]
                    } else {
                        Vec::new()
                    }
                });
            let source = fs::read_to_string(&path).unwrap_or_else(|error| {
                eprintln!("Could not read `{path}`: {error}");
                std::process::exit(1);
            });
            let demo = build_demo(&source, &inputs).unwrap_or_else(|error| {
                eprintln!("Could not prepare TUI demo: {error}");
                std::process::exit(1);
            });
            if let Err(error) = run_tui(demo) {
                eprintln!("TUI failed: {error}");
                std::process::exit(1);
            }
        }
        "verify" => {
            let Some(path) = env::args().nth(2) else {
                eprintln!("Usage: codeopt verify <file.tac> [inputs]");
                std::process::exit(2);
            };
            let inputs = env::args()
                .nth(3)
                .map_or_else(Vec::new, |raw_inputs| parse_inputs(&raw_inputs));
            let source = fs::read_to_string(&path).unwrap_or_else(|error| {
                eprintln!("Could not read `{path}`: {error}");
                std::process::exit(1);
            });
            let program = parse_program(&source).unwrap_or_else(|error| {
                eprintln!("Invalid TAC: {error}");
                std::process::exit(1);
            });
            let result = verify(&program, &inputs).unwrap_or_else(|error| {
                eprintln!("Verification failed: {error}");
                std::process::exit(1);
            });
            println!("Original output: {:?}", result.original_output);
            println!("Optimized output: {:?}", result.optimized_output);
            if result.matches() {
                println!("PASS: outputs match");
            } else {
                println!("FAIL: outputs differ");
                std::process::exit(1);
            }
        }
        other => {
            eprintln!("Unknown command: {other}. Run `codeopt help`.");
            std::process::exit(2);
        }
    }
}

fn parse_inputs(raw_inputs: &str) -> Vec<i32> {
    raw_inputs
        .split(',')
        .map(|token| {
            token.trim().parse::<i32>().unwrap_or_else(|_| {
                eprintln!("Invalid input integer: `{token}`");
                std::process::exit(2);
            })
        })
        .collect()
}

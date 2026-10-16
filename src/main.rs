use std::env;
use std::fs;

use codeopt::cfg::build;
use codeopt::parser::parse_program;
use codeopt::passes::optimize_with_worklist;
use codeopt::tui::{BenchmarkCorpus, run_workbench};
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
                "Usage: codeopt [status | format <file> | cfg <file> | run <file> [inputs] | optimize <file> | verify <file> [inputs] | tui [file] [inputs] | c-backup <file.c>]"
            );
        }
        "format" => {
            let Some(path) = env::args().nth(2) else {
                eprintln!("Usage: codeopt format <file>");
                std::process::exit(2);
            };
            let source = read_and_lower_source(&path);
            let program = parse_program(&source).unwrap_or_else(|error| {
                eprintln!("Invalid TAC: {error}");
                std::process::exit(1);
            });
            println!("{program}");
        }
        "cfg" => {
            let Some(path) = env::args().nth(2) else {
                eprintln!("Usage: codeopt cfg <file>");
                std::process::exit(2);
            };
            let source = read_and_lower_source(&path);
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
                eprintln!("Usage: codeopt run <file> [inputs]");
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
            let source = read_and_lower_source(&path);
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
                eprintln!("Usage: codeopt optimize <file>");
                std::process::exit(2);
            };
            let source = read_and_lower_source(&path);
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
            let corpus = BenchmarkCorpus::load_from_dir("benchmarks");
            if let Err(error) = run_workbench(corpus) {
                eprintln!("TUI failed: {error}");
                std::process::exit(1);
            }
        }
        "verify" => {
            let Some(path) = env::args().nth(2) else {
                eprintln!("Usage: codeopt verify <file> [inputs]");
                std::process::exit(2);
            };
            let inputs = env::args()
                .nth(3)
                .map_or_else(Vec::new, |raw_inputs| parse_inputs(&raw_inputs));
            let source = read_and_lower_source(&path);
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
        "c-backup" => {
            let Some(path) = env::args().nth(2) else {
                eprintln!("Usage: codeopt c-backup <file.c>");
                std::process::exit(2);
            };
            let tac_source = codeopt::c_frontend::lower_c_via_gcc_backup(&path).unwrap_or_else(|error| {
                eprintln!("GCC GIMPLE backup lowerer failed: {error}");
                std::process::exit(1);
            });
            println!("Extracted TAC from GCC GIMPLE:\n{tac_source}\n");
            let mut program = parse_program(&tac_source).unwrap_or_else(|error| {
                eprintln!("Invalid TAC produced by GIMPLE lowerer: {error}");
                std::process::exit(1);
            });
            let cfg = build(&mut program).unwrap_or_else(|error| {
                eprintln!("Invalid control flow: {error}");
                std::process::exit(1);
            });
            let report = optimize_with_worklist(&mut program, &cfg);
            println!("Optimization Report ({} passes fired):", report.transformations.len());
            for transformation in report.transformations {
                println!(
                    "{:?} [instruction {}]: {} => {}",
                    transformation.pass,
                    transformation.instruction_id,
                    transformation.before,
                    transformation.after
                );
            }
            println!("\nOptimized TAC:\n{program}");
        }
        other => {
            eprintln!("Unknown command: {other}. Run `codeopt help`.");
            std::process::exit(2);
        }
    }
}

fn read_and_lower_source(path: &str) -> String {
    let raw = fs::read_to_string(path).unwrap_or_else(|error| {
        eprintln!("Could not read `{path}`: {error}");
        std::process::exit(1);
    });
    if path.ends_with(".c") || codeopt::c_frontend::looks_like_c_code(&raw) {
        codeopt::c_frontend::lower_c_to_tac(&raw).unwrap_or_else(|error| {
            eprintln!("C frontend lowering failed: {error}");
            std::process::exit(1);
        })
    } else {
        raw
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

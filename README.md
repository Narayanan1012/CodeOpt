# CodeOpt

Course project for a rule-based optimizer over a small three-address-code
(TAC) language.

## Milestone 1

The initial milestone provides the Rust project skeleton and the core typed
models for TAC, basic blocks/CFG, optimization-pass selection, VM execution,
parsing, and transformation logging. The next implementation tasks are TAC
parsing, basic-block construction, the VM, and the first optimization passes.

## Toolchain

Use the stable Rust channel. The project has an MSRV of Rust 1.96; development
should use the latest installed stable release. The Rust edition is 2024, which
is a language-edition setting and is independent of the Rust compiler version.

```bash
cargo run -- status
```

## Review 1 scope

- Native `.tac` input
- 32-bit integer arithmetic
- Basic blocks and CFG
- Constant folding, propagation, algebraic simplification, local CSE
- Intra-block def-use worklist
- Differential VM verification
- A small TUI with TAC, transformation-log, and basic-block/CFG views

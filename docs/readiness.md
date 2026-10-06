# Readiness

This document gives the commands of this repository. Run each command from the root of the repository.

## Commands

| Purpose | Command |
| --- | --- |
| Format check | `cargo fmt --all -- --check` |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` |
| Test | `cargo test --workspace` |
| Build | `cargo build --workspace` |
| Run the game | `cargo run` |

## Before you open a pull request

1. Run the format check.
2. Run the lint.
3. Run the test.

If a command fails, correct the cause. Then run all three commands again.

## The workspace flag

The `--workspace` flag reads every crate of the repository. CI reads every crate. Do not remove the flag.

## Formatting

If the format check fails, run `cargo fmt --all`. Then run the format check again.

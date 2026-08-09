# rsnug

A secrets manager for use by AI agents.

**Currently only the interface is defined; no subcommand is implemented yet.** Running any of them prints a message to stderr and exits with code 1.

## Usage

```
rsnug [OPTIONS] <COMMAND>

Commands:
  init  Create a new vault
  set   Set a secret
  get   Get metadata for a secret
  list  List the registered keys

Options:
  -f, --vault <PATH>     Path to the vault file
      --format <FORMAT>  Output format [default: text] [possible values: text, json]
  -h, --help             Print help
  -V, --version          Print version
```

```
rsnug init [--force]
rsnug set <KEY> <VALUE>
rsnug set <KEY> --stdin
rsnug get <KEY> [--reveal]
rsnug list
```

`--vault` and `--format` are global options and can also be written after the subcommand.

## Contract with agents

This tool is designed for AI agents to call, ahead of interactive human use. The following are fixed promises, part of the interface itself.

### Never prompts interactively

No code path shows a prompt. There is no branching based on whether a TTY is attached. The value for `set` must always be given explicitly, either as a positional argument or via `--stdin`. Giving both, or giving neither, exits immediately with a usage error.

As a result, running the tool without connecting standard input never hangs waiting for input.

### stdout is payload-only

All diagnostic messages and errors go to stderr. stdout carries only the command's result — a single JSON value when `--format json` is given. stdout is empty on failure, so "empty output plus a non-zero exit code" can uniformly be treated as an error.

### Exit codes

| code | meaning |
| --- | --- |
| 0 | success |
| 1 | general error (including an unimplemented subcommand) |
| 2 | usage error (missing/conflicting arguments, unknown command) |
| 3 | key does not exist |
| 4 | vault is uninitialized, or authentication failed |

### `get` hides the value by default

`get <KEY>` returns only metadata such as whether the key exists, not the value itself. Pass `--reveal` explicitly if you need the plaintext.

Printing a plaintext value to stdout via an agent leaves that value in the LLM's context and conversation logs. Defaulting to non-exposure makes exposure always an explicit choice by the caller. For the same reason, use `--stdin` instead of the positional argument if you don't want the value to end up in shell history or `ps`.

## Development

```
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

To run inside a container, use `docker compose run --rm dev <command>`.

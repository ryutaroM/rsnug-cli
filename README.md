# rsnug

A secrets manager for use by AI agents.

Each vault is a single file, encrypted with [age](https://age-encryption.org/) using a passphrase read from the `RSNUG_PASSPHRASE` environment variable.

## Installation

```
brew tap ryutaroM/rsnug
brew trust --formula ryutarom/rsnug/rsnug
brew install rsnug
```

The `brew trust` step is required because this is a third-party tap; Homebrew otherwise refuses to load its formulae. See [Tap Trust](https://docs.brew.sh/Tap-Trust).

## Setup

rsnug reads its vault passphrase from the `RSNUG_PASSPHRASE` environment variable — it is never prompted for. Add it to your shell profile (`~/.zshrc` or `~/.bashrc`) so it's set in every session:

```
export RSNUG_PASSPHRASE="your-passphrase"
```

Then reload the shell (`source ~/.zshrc`, or open a new terminal) before running `rsnug init`.

## Usage

```
rsnug [OPTIONS] <COMMAND>

Commands:
  init     Create a new vault
  set      Set a secret
  get      Get metadata for a secret
  unset    Move a secret to the trash
  restore  Bring a trashed secret back
  list     List the registered keys

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
rsnug unset <KEY>
rsnug restore <KEY> [--at <TIMESTAMP>]
rsnug list [--trash]
```

`--vault` and `--format` are global options and can also be written after the subcommand. If `--vault` is omitted, the vault path defaults to `$XDG_CONFIG_HOME/rsnug/vault.age`, falling back to `$HOME/.config/rsnug/vault.age`.

## Contract with agents

This tool is designed for AI agents to call, ahead of interactive human use. The following are fixed promises, part of the interface itself.

### Never prompts interactively

No code path shows a prompt. There is no branching based on whether a TTY is attached. The value for `set` must always be given explicitly, either as a positional argument or via `--stdin`. Giving both, or giving neither, exits immediately with a usage error.

As a result, running the tool without connecting standard input never hangs waiting for input.

### Passphrase comes from the environment

Every command that touches the vault (including `init`) reads the passphrase from `RSNUG_PASSPHRASE`. If it is unset or empty, the command exits with code 4 without touching the filesystem.

### stdout is payload-only

All diagnostic messages and errors go to stderr. stdout carries only the command's result — a single JSON value when `--format json` is given. stdout is empty on failure, so "empty output plus a non-zero exit code" can uniformly be treated as an error.

### Exit codes

| code | meaning |
| --- | --- |
| 0 | success |
| 1 | general error (I/O failure, `init` without `--force` on an existing vault, `restore` onto a live key, etc.) |
| 2 | usage error (missing/conflicting arguments, unknown command) |
| 3 | key (or trashed generation) does not exist |
| 4 | vault is uninitialized, authentication failed, or `init --force` was pointed at a vault the passphrase does not open |

### Losing a secret always takes two commands

No single command can destroy a value that is not already recoverable by another one.

`unset` does not destroy anything. It moves the entry out of the vault's live set and into a trash the same vault file carries. `restore <KEY>` puts it back; `list --trash` shows what is in there, as key names and deletion timestamps only — never values.

`set` behaves the same way when it overwrites: if the key is already live, the previous value is pushed onto the trash before the new one is stored. Overwriting by accident is therefore as recoverable as unsetting by accident. Re-setting a key to the value it already holds is a no-op for the trash — nothing would be lost, so nothing is stacked.

There is deliberately no command that empties the trash. An agent holding the passphrase can enumerate every key with `list` and call `unset` or `set` on each one, and no amount of that can lose a secret. Since the tool never prompts, this is the only way to make destruction safe — not by asking, but by making it undoable.

Trashing the same key twice stacks generations rather than overwriting, so `set K a; set K b; unset K` leaves both values in the trash. `restore <KEY>` returns the most recent one. To pick an older one, pass `--at` with the RFC3339 timestamp `list --trash` prints for it, copied verbatim — the listing is the index of what is restorable, and every line in it can be restored.

Restoring an older generation leaves the newer ones in the trash. If several generations of a key share the same second, `--at` returns the newest of them; `unset` the key again and repeat to reach the ones behind it. A well-formed timestamp with no generation behind it exits 3, the same as a key that does not exist; a string that is not RFC3339 is a usage error and exits 2.

`restore` refuses to overwrite a key that is currently live, exiting 1 instead of replacing it.

`init --force` is the one command that does discard the trash, so it verifies ownership first: if the existing vault cannot be decrypted with `RSNUG_PASSPHRASE`, it refuses and exits 4 without touching the file. Knowing the path to a vault is not enough to erase it — you have to be able to open it. If the file is genuinely corrupt and no passphrase opens it, rsnug will not recreate it for you; delete the file yourself and run `init` again.

The cost of all this is that `unset` is not a way to shred a compromised secret. The value stays in the vault file, encrypted, until the vault itself is replaced with `init --force` by someone holding the passphrase.

### Vault format

The vault records a format version. A vault with an empty trash is written as version 1, byte-compatible with rsnug releases that predate `unset`. It is upgraded to version 2 only once something is actually in the trash, and drops back to version 1 when the trash empties again. Installing this version therefore does not by itself stop an older rsnug binary from reading your vault — putting something in the trash does, whether via `unset` or via a `set` that overwrites a live key. Older binaries reject a version 2 vault outright rather than silently discarding its trash.

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

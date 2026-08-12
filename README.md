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
  init   Create a new vault
  set    Set a secret
  get    Get metadata for a secret
  unset  Delete a secret
  list   List the registered keys

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
rsnug list
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
| 1 | general error (I/O failure, `init` without `--force` on an existing vault, etc.) |
| 2 | usage error (missing/conflicting arguments, unknown command) |
| 3 | key does not exist |
| 4 | vault is uninitialized, authentication failed, or `init --force` was pointed at a vault the passphrase does not open |

### Deletion is permanent

`unset <KEY>` deletes the entry. There is no undo and no command that brings it back. rsnug writes a new vault file without the entry and renames it over the old one, so the value is gone from the vault rsnug reads from here on.

It is not a shredder. The replaced file's blocks are unlinked rather than overwritten, so the old ciphertext can survive in free space, in backups, and in filesystem snapshots — and it opens with the same passphrase. To retire a compromised secret, rotate it at the source; `unset` only stops rsnug from handing it out.

`set` on a key that already exists replaces the value outright, with the same finality.

Since the tool never prompts, an agent holding the passphrase can enumerate every key with `list` and destroy the vault's contents in a loop. Guard the passphrase accordingly, and keep a backup of the vault file if the secrets in it are not reproducible.

`unset` on a key that does not exist changes nothing and exits 3. It is not idempotent by design: a caller that expected the key to be there finds out.

### `init --force` verifies ownership

`init --force` replaces an existing vault with an empty one, so it checks first that `RSNUG_PASSPHRASE` actually decrypts the file. If it does not, the command refuses and exits 4 without touching a byte. Knowing the path to a vault is not enough to erase it — you have to be able to open it.

If the file is genuinely corrupt and no passphrase opens it, rsnug will not recreate it for you; delete the file yourself and run `init` again.

### Vault format

The vault records a format version, currently 1. A vault carrying any other version is rejected rather than read on a guess.

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

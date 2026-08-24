# rsnug

A secrets manager for use by AI agents.

Each vault is a single file, encrypted with [age](https://age-encryption.org/) to an X25519 identity kept in a key file that you own and back up yourself.

## Installation

```
brew tap ryutaroM/rsnug
brew trust --formula ryutarom/rsnug/rsnug
brew install rsnug
```

The `brew trust` step is required because this is a third-party tap; Homebrew otherwise refuses to load its formulae. See [Tap Trust](https://docs.brew.sh/Tap-Trust).

## Setup

There is nothing to configure. `rsnug init` generates a key file at `$XDG_CONFIG_HOME/rsnug/key` (falling back to `$HOME/.config/rsnug/key`) the first time you run it, with mode `600`:

```
rsnug init
```

There is no passphrase and nothing to memorize. The key file is the only thing that opens the vault.

### Back up the key file

**Lose the key file and the vault is gone for good.** There is no recovery code and no second copy. Back it up the way you would an SSH private key:

```
cp ~/.config/rsnug/key /path/to/somewhere/safe
```

To use the same vault on another machine, copy both the vault and the key file to the same paths there. The key file is a standard age identity, so `age-keygen`-produced keys work too.

### Keep the key out of sync and version control

The key file sits next to the vault in `~/.config/rsnug/`. If you sync that directory to Dropbox, iCloud, or a dotfiles repository, **the key travels with the ciphertext and the encryption stops meaning anything.** rsnug cannot detect this. Either keep `~/.config/rsnug/` out of the sync, or move the key elsewhere with `--key-file`:

```
# in a dotfiles repo
echo '.config/rsnug/' >> .gitignore
```

### One key file, several vaults

A key file holds any number of identities, the same way an age `keys.txt` does. `rsnug init` reuses the first one, so several vaults share a key by default:

```
rsnug -f ~/work.age init
rsnug -f ~/home.age init
```

Pass `--new-key` to give a vault its own identity instead. It appends to the key file and never rewrites what is already there:

```
rsnug -f ~/work.age init --new-key
```

Either way one key file opens all of them, so there is no pairing to remember.

## Usage

```
rsnug [OPTIONS] <COMMAND>

Commands:
  init     Create a new vault
  set      Set a secret
  get      Get metadata for a secret
  unset    Delete a secret
  list     List the registered keys
  migrate  Re-encrypt a passphrase vault to the key file

Options:
  -f, --vault <PATH>     Path to the vault file
      --key-file <PATH>  Path to the age key file
      --format <FORMAT>  Output format [default: text] [possible values: text, json]
  -h, --help             Print help
  -V, --version          Print version
```

```
rsnug init [--force] [--new-key]
rsnug set <KEY> <VALUE>
rsnug set <KEY> --stdin
rsnug get <KEY> [--reveal]
rsnug unset <KEY>
rsnug list
rsnug migrate
```

`--vault`, `--key-file` and `--format` are global options and can also be written after the subcommand. If `--vault` is omitted, the vault path defaults to `$XDG_CONFIG_HOME/rsnug/vault.age`, falling back to `$HOME/.config/rsnug/vault.age`.

## Migrating a passphrase vault

Vaults created before this scheme were encrypted with `RSNUG_PASSPHRASE`. Running any ordinary command on one exits 4 and tells you to migrate — it is not mistaken for a wrong key, because the encryption method is readable from the file header without decrypting it.

```
RSNUG_PASSPHRASE="your-old-passphrase" rsnug migrate
```

This decrypts with the passphrase, generates the key file if you do not have one, copies the vault to `<vault>.age.bak`, and re-encrypts to the key. A migrate that stops before rewriting the vault leaves no backup behind, so it is safe to fix whatever it complained about and run it again. The backup is left in place; delete it yourself once you have confirmed the migration, and remove `RSNUG_PASSPHRASE` from your shell profile.

## Contract with agents

This tool is designed for AI agents to call, ahead of interactive human use. The following are fixed promises, part of the interface itself.

### Never prompts interactively

No code path shows a prompt. There is no branching based on whether a TTY is attached. The value for `set` must always be given explicitly, either as a positional argument or via `--stdin`. Giving both, or giving neither, exits immediately with a usage error.

As a result, running the tool without connecting standard input never hangs waiting for input.

This holds under the key file scheme too. rsnug reads the key from a file and never asks anyone for it, so there is no OS keychain in the path and no dialog to answer — the reason a key file was chosen over the system keyring.

### The key comes from a file

Every command that touches the vault reads the key from a file, resolved in this order:

1. `--key-file <PATH>`
2. the `RSNUG_KEY_FILE` environment variable, if set and non-empty
3. `$XDG_CONFIG_HOME/rsnug/key`
4. `$HOME/.config/rsnug/key`

`init` creates that file if it is missing. Every other command exits 4 if it is missing.

The file must not be readable by group or other. rsnug refuses a key file with any of those bits set and exits 4 rather than using it, the way `ssh` does.

### Any age tool can open the vault

The vault is a plain age file encrypted to an X25519 recipient, and the key file is a plain age identity. If rsnug is unavailable or you no longer trust it, the vault opens with the reference implementation:

```
age -d -i ~/.config/rsnug/key ~/.config/rsnug/vault.age
```

Nothing about the format is specific to rsnug, so a vault is never held hostage by this tool.

### stdout is payload-only

All diagnostic messages and errors go to stderr. stdout carries only the command's result — a single JSON value when `--format json` is given. stdout is empty on failure, so "empty output plus a non-zero exit code" can uniformly be treated as an error.

### Exit codes

| code | meaning |
| --- | --- |
| 0 | success |
| 1 | general error (I/O failure, `init` without `--force` on an existing vault, etc.) |
| 2 | usage error (missing/conflicting arguments, unknown command) |
| 3 | key does not exist |
| 4 | vault is uninitialized, the key file is missing/loose-permissioned/malformed, the key does not open the vault, or the vault still uses a passphrase and needs `migrate` |
| 5 | another rsnug process holds the vault lock and the wait timed out (retry) |

### Concurrent writes are serialized

Every command that changes the vault — `set`, `unset`, `init`, `migrate` — holds an exclusive lock across its whole read-modify-write, so two rsnug processes cannot interleave. Without it, a `set` and an `unset` started at the same time both exit 0 and one of the two changes is silently lost: whichever saves last writes a snapshot it read before the other one landed, resurrecting a deleted key or dropping a written one.

The lock is an empty sidecar file next to the vault, `<vault>.lock`, held with `flock(2)` (`LockFileEx` on Windows). The kernel releases it when the process exits, so a crashed or killed rsnug never strands the vault and there is no stale lock to clear by hand. The file itself is never deleted — unlinking it would let the next process lock a different inode — so `vault.age.lock` stays beside `vault.age` for good. It holds nothing; removing it while no rsnug is running is harmless.

A command that cannot take the lock retries for 5 seconds and then exits 5 instead of hanging. A vault write takes milliseconds, so exhausting that wait means an unusual pile-up of writers, and exit 5 is always worth retrying.

`get` and `list` take no lock. They never block behind a writer and never fail with exit 5. A write is published by renaming a finished file over the old one, so a reader always sees one whole vault, never a half-written one.

The guarantee is per command, not per caller. An agent that reads with `get` and then writes with `set` still has a gap between the two calls, and rsnug cannot close it; if two agents update the same key, coordinate them above rsnug.

### Deletion is permanent

`unset <KEY>` deletes the entry. There is no undo and no command that brings it back. rsnug writes a new vault file without the entry and renames it over the old one, so the value is gone from the vault rsnug reads from here on.

It is not a shredder. The replaced file's blocks are unlinked rather than overwritten, so the old ciphertext can survive in free space, in backups, and in filesystem snapshots — and it opens with the same key. To retire a compromised secret, rotate it at the source; `unset` only stops rsnug from handing it out.

`set` on a key that already exists replaces the value outright, with the same finality.

Since the tool never prompts, an agent that can read the key file can enumerate every key with `list` and destroy the vault's contents in a loop. Guard the key file accordingly, and keep a backup of the vault file if the secrets in it are not reproducible.

`unset` on a key that does not exist changes nothing and exits 3. It is not idempotent by design: a caller that expected the key to be there finds out.

### `init --force` verifies ownership

`init --force` replaces an existing vault with an empty one, so it checks first that the key file actually decrypts it. If it does not — including when there is no key file at all — the command refuses and exits 4 without touching a byte. Knowing the path to a vault is not enough to erase it; you have to be able to open it.

If the file is genuinely corrupt and no key opens it, rsnug will not recreate it for you; delete the file yourself and run `init` again.

`--force` applies to the vault only. **`init` never overwrites a key file**, with or without `--force`, because doing so would strand every vault that key opens.

### Vault format

The vault records a format version, currently 1. A vault carrying any other version is rejected rather than read on a guess. The version did not change when vaults moved from passphrases to key files: the contents are identical, and only the age recipient differs — which is legible from the file header without a key.

### `get` hides the value by default

`get <KEY>` returns only metadata such as whether the key exists, not the value itself. Pass `--reveal` explicitly if you need the plaintext.

Printing a plaintext value to stdout via an agent leaves that value in the LLM's context and conversation logs. Defaulting to non-exposure makes exposure always an explicit choice by the caller. For the same reason, use `--stdin` instead of the positional argument if you don't want the value to end up in shell history or `ps`.

## Development

```
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

One test is marked `ignore` because it encrypts and decrypts at scrypt work factor 20, which takes minutes in a debug build. It guards the migration path against a vault written on a machine much faster than the one reading it. Run it before touching `decrypt_legacy`:

```
cargo test -- --ignored
```

To run inside a container, use `docker compose run --rm dev <command>`.

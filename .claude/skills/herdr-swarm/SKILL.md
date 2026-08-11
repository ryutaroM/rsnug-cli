---
name: herdr-swarm
description: "Delegate independent, parallelizable tasks to Claude Code workers via herdr. Use when the user explicitly asks to use herdr for swarm/parallel delegation, or when there are multiple independent subtasks worth delegating. Do not use just because background work could help. Requires HERDR_ENV=1."
argument-hint: "[tasks to delegate, optional]"
allowed-tools: Bash(herdr agent list), Bash(herdr agent read *), Bash(herdr agent start *), Bash(herdr agent prompt *), Bash(herdr agent wait *), Bash(herdr agent send-keys *), Bash(herdr worktree list *), Bash(herdr worktree create *), Bash(herdr pane current *), Bash(herdr pane rename *), Bash(herdr integration status), Bash(printenv HERDR_ENV), Read
---

# herdr swarm

Run independent subtasks as Claude Code workers, each in its own git worktree, and report their results back.

## Preflight
- HERDR_ENV: !`printenv HERDR_ENV`
- caller pane: !`herdr pane current --current 2>&1`
- integrations: !`herdr integration status 2>&1`
- worktrees: !`herdr worktree list 2>&1`
- live agents: !`herdr agent list 2>&1`

## Tasks to delegate
$ARGUMENTS

## Instructions

If HERDR_ENV is not `1`, stop and tell the user this must run inside a herdr-managed pane.

If the `claude:` line of the integration status says "not installed", lifecycle detection is unreliable: `agent_status` can report `idle` while an interactive dialog is actually on screen. Don't install it yourself — confirm with `--source visible` reads instead of trusting status alone.

### Plan the swarm

Identify independent subtasks from $ARGUMENTS or the current conversation — only delegate when tasks are genuinely parallelizable.

The preflight listing is a load-time snapshot. Re-read it right before starting workers:

```bash
herdr agent list
```

Read `agent`, `agent_status`, `pane_id`, and `cwd` from the JSON. A swarm worker is a `claude` agent whose `cwd` is under `<repo root>/.claude/worktrees/`, where the repo root is `.result.source.repo_root` from `worktree list`. Every other agent — the caller's own pane and any session the user is driving at the repo root — is off limits: never count it, reuse it, prompt it, or send keys to it. Identify the caller by the `pane_id` that `pane current --current` reports, not by `$HERDR_PANE_ID`: the environment variable is captured at pane creation and goes stale when the session is re-created.

Reuse a swarm worker that is `idle` or `done` — with `--no-focus` background work, a finished worker normally reports `done`, not `idle`.

Cap concurrent swarm workers in this repo at 3, counting workers already `working`/`blocked` toward the limit. Queue extra subtasks and start a queued one only when a running worker settles — a returning `agent wait` is the signal, so never poll on a timer.

### Name a worker

Pick one short `<name>` per worker matching `[a-z][a-z0-9_-]{0,31}`, unique among live agents, and reuse it for the worktree label, pane label, and agent name. Derive it from the subtask (`parser`, `docs-brew`), not from a branch name — branch names usually contain `/` and are invalid as agent names.

The branch is `swarm/<name>` unless the user names another prefix or a full branch name.

### Start a worker

Base the branch on `main` unless the user asks for another base:

```bash
herdr worktree create --cwd "$PWD" --branch swarm/<name> --base main \
  --path .claude/worktrees/<name> --label <name> --no-focus
```

This creates a new workspace, tab, and root pane. Take the pane ID from `.result.root_pane.pane_id` in the JSON response, label the pane itself (`--label` only names the workspace/tab), then start the agent with a startup timeout large enough for a first launch. Pass no native agent arguments — the worker runs with its default permission behavior:

```bash
herdr pane rename <pane_id> <name>
herdr agent start <name> --kind claude --pane <pane_id> --timeout 60000
```

A worker asks whether it trusts the files in a directory the first time it starts in a new worktree path, and herdr may not detect that as `blocked`. Check before sending the real task:

```bash
herdr agent read <name> --source visible --lines 40
```

If the dialog is present, answer from what you just read — pick the option that simply proceeds, not one that grants anything beyond this directory. Usually that is the pre-selected first option:

```bash
herdr agent send-keys <name> enter
```

### Send the task

The worker can't see this conversation or any plan file, and its worktree contains only the repo's tracked files — untracked project instructions never reach it. Every prompt must be self-contained and state:

- the goal and its definition of done
- the exact paths/files in scope
- project conventions it must follow: write the failing test first and drive the change through red → green → refactor; keep comments to a minimum
- whether to commit in its worktree (say so explicitly; it never pushes or opens a PR)
- what it must not do: work outside its own worktree, push, or read or write secrets and credential files

```bash
herdr agent prompt <name> "<self-contained prompt>" --wait --timeout 600000
```

If this returns `agent_prompt_stalled`, the text likely landed in the input box without submitting (seen right after dismissing the trust dialog) — send `herdr agent send-keys <name> enter` and retry the wait.

### Wait on workers

A single wait must not exceed 600000 ms, and the Bash call's own timeout must be set to match — otherwise the tool call is killed while the worker keeps running and its result is lost. For longer work, repeat:

```bash
herdr agent wait <name> --timeout 600000
```

Wait on several workers in parallel (independent Bash calls in one message), or send all prompts without `--wait` and then wait on each. Do not serialize a blocking wait per worker.

Give one worker at most two consecutive full-timeout waits (~20 minutes). If it still has not settled, stop waiting on it, leave it running, and report it to the user with what `--source visible` shows — never loop on it indefinitely, and never let a stuck worker block the queue: treat its slot as occupied and continue with the workers that did settle.

Handle the settled state:

- `blocked`: read `--source visible`. For a permission confirmation, approve the single action by default. Only when the same confirmation keeps recurring and the action stays inside the worktree, choose the option that remembers it for the session or project — that is written into the worktree's own local settings and dies with the worktree, but report it in the final summary. Never choose an option that turns permission checks off wholesale. For a genuine question, respond based on content.
- `unknown`: not proof of completion. Read `--source visible` and decide; never report it as done.
- `idle`/`done`: read the result.

### Read the result

```bash
herdr agent read <name> --source recent-unwrapped --lines 120
```

If raising `--lines` reveals no more of a completed response, the agent is on the terminal's alternate screen and those rows are unrecoverable — expect this to be the normal case for a worker running a full-screen TUI. Then ask the worker to write its full response as Markdown into a temporary directory and reply with just the path, and read that file. Never request file output in the initial prompt.

### Wrap up

Summarize each worker's results for the user, and list what was created so they can clean up: `<name>`, branch, worktree path, and workspace ID.

Out of scope: pushing, `gh pr create`, merging to main, worktree cleanup — leave these to the user or the existing project workflow.

Safety: use `--no-focus` for background work; never run bare `herdr` (it launches or attaches the TUI); never close panes/tabs/workspaces you didn't create; never run `herdr server stop` unless explicitly asked.

`agent send-keys` is the one command here that types into a worker's terminal, so keep it minimal: send only the keys needed to answer a dialog you just read with `--source visible`, only to a worker this run started, and never a key that would interrupt or exit the agent. Task text always goes through `agent prompt`, never through keystrokes.

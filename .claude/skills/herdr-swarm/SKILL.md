---
name: herdr-swarm
description: "Lead a swarm of Claude Code bees via herdr: one worktree and one bee per role, dispatched from a single queen pane, with each bee reporting back to the queen when it finishes. Use when the user explicitly asks to use herdr for swarm/parallel delegation, or when there are multiple independent subtasks worth delegating. Do not use just because background work could help. Requires HERDR_ENV=1."
argument-hint: "[tasks to delegate, optional]"
allowed-tools: Bash(herdr agent list), Bash(herdr agent get *), Bash(herdr agent read *), Bash(herdr agent start *), Bash(herdr agent prompt *), Bash(herdr agent rename *), Bash(herdr worktree list *), Bash(git worktree add *), Bash(herdr tab create *), Bash(herdr agent send-keys *), Bash(herdr workspace list), Bash(herdr pane current *), Bash(herdr integration status), Bash(printenv HERDR_ENV), Read
---

# herdr swarm

You are the **queen**. Split the work into roles, give each role its own worktree and **bee**
inside your own workspace, then stop. Bees prompt their results back to you; you never poll for
them.

Beyond building the bee's tab, do not build your own scaffolding around herdr's commands — no
scratch directories, no report files, no hook settings, no manifest. herdr's own state
(`agent list`, `workspace list`) is the record.

## Preflight
- HERDR_ENV: !`printenv HERDR_ENV`
- caller pane: !`herdr pane current --current 2>&1`
- integrations: !`herdr integration status 2>&1`
- worktrees: !`herdr worktree list 2>&1`
- live agents: !`herdr agent list 2>&1`

## Tasks to delegate
$ARGUMENTS

## 1. Become the queen

If HERDR_ENV is not `1`, stop and tell the user this must run inside a herdr-managed pane.

Take `.result.pane.pane_id` and `.result.pane.workspace_id` from `herdr pane current --current`
— not `$HERDR_PANE_ID` / `$HERDR_WORKSPACE_ID`, which go stale — and claim a stable name:

```bash
herdr agent rename <queen_pane_id> queen
```

If `queen` is taken by another pane, use `queen-<suffix>`. Bees address you by this name, so use
it verbatim in every prompt. Keep `<queen_workspace_id>` too: every bee's tab opens inside it.

## 2. Plan the roles

Identify genuinely parallelizable roles from $ARGUMENTS or the conversation. One role, one
worktree, one bee. Cap concurrent bees at 3 per repo; queue the rest and release one when a
report frees a slot.

Re-read `herdr agent list` (preflight is a load-time snapshot). Drop your own `pane_id` first,
then a bee is a `claude` agent whose `cwd` is under `<repo_root>/.claude/worktrees/`, where
`repo_root` is `.result.source.repo_root` from `herdr worktree list`. Every other agent —
including the user's own session — is off limits: never count, reuse, prompt, or key it.

Name each role with a short `<name>` matching `[a-z][a-z0-9_-]{0,31}`, unique among live agents,
derived from the role (`parser`, `docs-brew`) rather than a branch name. Reuse it for the
worktree label and the agent name. Branch is `swarm/<name>` unless the user says otherwise.
`bee` is the collective noun, never an individual name.

## 3. Start a bee

`herdr worktree create` (and `worktree open`) always opens its **own new top-level workspace** —
`--workspace` on those commands does not attach to an existing one, it is silently ignored for
that purpose. Using either would scatter each bee into its own workspace and bury the queen, so
build the checkout with plain git instead and open it as a tab inside your own workspace. Give
git an **absolute** path — a relative one resolves against your own cwd, which nests the bee's
worktree inside yours when you are yourself in a worktree. Base the branch on `main` unless the
user asks for another base:

```bash
git worktree add -b swarm/<name> <repo_root>/.claude/worktrees/<name> main

herdr tab create --workspace <queen_workspace_id> \
  --cwd <repo_root>/.claude/worktrees/<name> --label <name> --no-focus
```

Take the pane from `.result.root_pane.pane_id` and the tab from `.result.tab.tab_id` (you need
the tab ID to clean up). Then start the agent there, passing nothing that changes permission
behavior:

```bash
herdr agent start <name> --kind claude --pane <pane_id> --timeout 60000
```

A first start in a new worktree path raises a trust dialog that herdr may not report as
`blocked`, and a bee sitting on it can never report. Check before dispatching:

```bash
herdr agent read <name> --source visible --lines 40
```

If the dialog is up, do not dispatch and do not send keys — name the bee, quote the screen, give
the user the pane ID, and move on to the other roles. You will pick it up on a later sweep.

## 4. Dispatch the role

The bee cannot see this conversation, and its worktree holds only tracked files — untracked
project instructions never reach it. Every prompt must be self-contained and state:

- the goal and its definition of done
- the exact paths and files in scope
- project conventions: write the failing test first, drive it red → green → refactor, keep
  comments to a minimum
- whether to commit in its worktree (it never pushes or opens a PR)
- what it must not do: work outside its worktree, push, touch secrets or credential files, or
  start bees of its own
- the reporting contract below, with `<name>` and `<queen>` filled in

Embed verbatim:

> When you finish — whether the work is done or you are stuck — report by running exactly one
> command. It is the only channel back; nothing you print in your final response is read.
>
> ```bash
> herdr agent prompt <queen> "[swarm:<name>] STATUS: DONE — <one-line summary>
>
> <details: what changed, which files, what to check>"
> ```
>
> Use `STATUS: FAILED — <why you could not finish>` or `STATUS: BLOCKED — <the decision you need
> from the queen>` in place of `DONE` when that is the truth. Keep the whole report inside the
> one quoted argument, and avoid unescaped double quotes in it.

Send it without waiting:

```bash
herdr agent prompt <name> "<self-contained prompt>"
```

On `agent_prompt_stalled`, read `--source visible` first. A dialog means the bee belongs to the
user; hand it over. Only if your prompt text sits unsubmitted with no dialog present, submit it
with `herdr agent send-keys <name> enter`.

## 5. Hand control back

Once every startable role is dispatched, **stop**. No `herdr agent wait`, no polling loop, no
timer. Tell the user which bees are flying, which roles are queued, and which are waiting on a
dialog only they can answer. A bee's report arrives in your pane as a new turn — that is the
only thing you wait on.

## 6. Handle a report

A turn starting with `[swarm:<name>]` is a bee reporting. Handle it in one pass:

1. Take the `STATUS:` line.
   `DONE` — record the outcome and free the slot.
   `FAILED` — re-dispatch, reassign, or surface it.
   `BLOCKED` — answer with `herdr agent prompt <name> "<answer>"` and keep the slot.
2. If a slot is free and roles are queued, start the next one now.
3. Sweep `herdr agent list` once — the only way to catch a bee that never reported. For a bee
   that is `blocked` or `unknown`, read `--source visible` and hand it to the user with its
   name, screen, and pane ID; `unknown` is unresolved, never done. For a bee that is `idle` or
   `done` with no report in hand, re-prompt it for the report — it ended its turn without
   running the command. A bee the user has unblocked shows up ready here; dispatch its role then.
4. Report the increment to the user, then stop again.

Screen reads are a fallback, not truth: `herdr agent read <name> --source recent-unwrapped
--lines 120`. If raising `--lines` reveals nothing more, the output is on the alternate screen
and unrecoverable — re-prompt for the report instead.

## 7. Wrap up

When every role has reported, summarize the results per role and list what was created:
`<name>`, branch, worktree path, tab ID. Spell out the cleanup — `git worktree remove <path>`
plus `herdr tab close <tab_id>` and `git branch -d swarm/<name>` once no longer wanted — but do
not run it. Pushing, `gh pr create`, and merging are the user's, too.

## Rules

- **Never answer a bee's dialog.** Approvals, trust prompts, permission confirmations: do not
  send `enter`, `y`, or an option number — not to unstick a queue, not because it looks
  harmless, not because the user approved something similar before. Surface it and stop.
- `agent send-keys` has exactly one use: `enter` to submit your own stalled prompt, after a
  `--source visible` read shows it in the box with no dialog, to a bee this run started. Never
  send keys that interrupt or exit an agent, and never send task text as keystrokes.
- Use `--no-focus` for background work. Never run bare `herdr` (it launches the TUI), never
  remove or close worktrees, workspaces, tabs, or panes you did not create, and never run
  `herdr server stop` unasked.
- If `herdr integration status` says the `claude:` integration is not installed, lifecycle
  detection is unreliable — `agent_status` can read `idle` with a dialog on screen. Do not
  install it yourself; confirm with `--source visible` reads.

---
name: herdr-swarm
description: "Lead a swarm of Claude Code bees via herdr: one worktree and one bee per role, dispatched from a single queen pane, with each bee reporting back to the queen when it finishes. Use when the user explicitly asks to use herdr for swarm/parallel delegation, or when there are multiple independent subtasks worth delegating. Do not use just because background work could help. Requires HERDR_ENV=1."
argument-hint: "[tasks to delegate, optional]"
allowed-tools: Bash(herdr agent list), Bash(herdr agent get *), Bash(herdr agent read *), Bash(herdr agent start *), Bash(herdr agent prompt *), Bash(herdr agent rename *), Bash(herdr agent wait *), Bash(herdr worktree list *), Bash(git worktree add *), Bash(herdr tab create *), Bash(herdr agent send-keys *), Bash(herdr workspace list), Bash(herdr pane current *), Bash(herdr integration status), Bash(printenv HERDR_ENV), Read
---

# herdr swarm

You are the **queen**. Split the work into roles, give each role its own worktree and **bee**
inside your own workspace, then hand control back. Bees prompt their results to you; you arm one
blocking wait rather than polling.

Beyond building the bee's tab, do not build your own scaffolding around herdr's commands — no
scratch directories, no report files, no hook settings, no manifest. `herdr agent list` is the
record: a bee's tab carries a `cwd` but no worktree identity, so `workspace list` shows no
worktree for it and `worktree list` shows the worktree with no workspace attached.

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

## 5. Hand control back, then arm one wait

A report is not guaranteed to arrive. It can be dropped in transit, or a bee can end its turn
without running the command. If you stop with bees outstanding and nothing else armed, a lost
report leaves nobody to wake you and the swarm deadlocks in silence.

So once every startable role is dispatched, do both, in this order:

1. **Tell the user first**, in text, before any further tool call: which bees are flying, which
   roles are queued, which are waiting on a dialog only they can answer. Close with a line
   telling them that a report can be lost, and that prompting you is the way to restart a swarm
   that has gone quiet — their prompt reaches you once the wait below returns.
2. **Arm exactly one blocking wait** — no loop, no timer, no repeated status reads. Pick the
   target from a fresh `herdr agent list`: a bee this run dispatched, never one you handed to the
   user, and whose `agent_status` is `working`. A bee still `idle` has not picked its prompt up
   yet, and one sitting on a dialog is already `blocked`; either matches the moment you arm and
   spins you straight back into the sweep. If no bee qualifies, arm nothing — go to step 6 and
   sweep instead.

   ```bash
   herdr agent wait <name> --timeout 540000
   ```

   Run it with the Bash tool's own `timeout` set to `600000`, its maximum. Left at the default
   the tool kills the call after two minutes, and you read your own kill as a herdr timeout — a
   two-minute poll wearing the costume of a blocking wait. Leave `--until` off: the default
   idle/done/blocked is what catches a bee ending its turn, and a claude bee returns to `idle`
   rather than `done` when it finishes.

   `agent wait` takes one target; the sweep in step 6 covers the rest. When it returns, go to
   step 6.

The wait holds you inside a tool call, so nothing reaches you while it runs: another bee's
report, even a `BLOCKED` one, and anything the user prompts both queue until it returns. That
latency is the price of never deadlocking on a lost report, and the nine-minute bound is what
keeps it payable — do not raise it. If no bee is outstanding, do not arm anything; stop.

## 6. Handle a report

You enter here two ways: a turn starting with `[swarm:<name>]`, which is a bee reporting, or the
step 5 wait returning. Either way, handle it in one pass — with no report in hand, skip 1 and run
2 through 4:

1. Take the `STATUS:` line.
   `DONE` — record the outcome and free the slot.
   `FAILED` — re-dispatch, reassign, or surface it.
   `BLOCKED` — answer with `herdr agent prompt <name> "<answer>"` and keep the slot.
2. If a slot is free and roles are queued, start the next one now.
3. Sweep `herdr agent list` once — the only way to catch a bee that never reported. For a bee
   that is `blocked` or `unknown`, read `--source visible` and hand it to the user with its
   name, screen, and pane ID; `unknown` is unresolved, never done. For a bee that is `idle` or
   `done` with no report in hand, read `--source visible` first: the report may be on screen but
   lost in transit, in which case take it from there. Otherwise re-prompt it for the report — it
   ended its turn without running the command. A bee the user has unblocked shows up ready here;
   dispatch its role then.
4. Report the increment to the user in text, then re-arm the step 5 wait, target rules and all.
   If no bee is outstanding, go to step 7.

   Count consecutive passes through this step that changed nothing — no report, no status moved,
   no slot freed — including a pass you reached because no bee qualified to be waited on. Any
   report, and any change the sweep finds, resets the count to zero. At two, stop instead of
   re-arming: tell the user which bees are still flying, and that a late report or a prompt from
   them wakes you.

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

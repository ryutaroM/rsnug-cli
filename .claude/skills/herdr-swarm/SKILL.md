---
name: herdr-swarm
description: "Lead a swarm of Claude Code bees via herdr: one worktree per role, dispatched from a single queen pane, with bees reporting completion and problems back to the queen. Use when the user explicitly asks to use herdr for swarm/parallel delegation, or when there are multiple independent subtasks worth delegating. Do not use just because background work could help. Requires HERDR_ENV=1."
argument-hint: "[tasks to delegate, optional]"
allowed-tools: Bash(herdr agent list), Bash(herdr agent get *), Bash(herdr agent read *), Bash(herdr agent start *), Bash(herdr agent prompt *), Bash(herdr agent rename *), Bash(herdr agent send-keys *), Bash(herdr worktree list *), Bash(herdr tab create *), Bash(herdr pane current *), Bash(herdr pane rename *), Bash(herdr integration status), Bash(git worktree add *), Bash(printenv HERDR_ENV), Bash(mktemp *), Read, Write
---

# herdr swarm

You are the **queen**. You own one pane, split the work into roles, give each role its own
worktree and **bee**, and then stop. Bees push their results back to you; you never poll for
them.

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

If the `claude:` line of the integration status says "not installed", lifecycle detection is
unreliable: `agent_status` can report `idle` while an interactive dialog is actually on
screen. Don't install it yourself — confirm with `--source visible` reads instead of trusting
status alone.

### Become the queen

Bees address you by agent name, so you need a stable one. Read the caller's own identity from
`herdr pane current --current` — `.result.pane.pane_id` and `.result.pane.workspace_id`, not
`$HERDR_PANE_ID` / `$HERDR_WORKSPACE_ID`, which are captured at pane creation and go stale or
empty when the session is re-created — and name that agent:

```bash
herdr agent rename <queen_pane_id> queen
```

If `queen` is already held by a pane that is not the caller, pick `queen-<suffix>` instead.
Use this name verbatim in every bee prompt. Keep `<queen_workspace_id>` too: every bee lives
in it.

### Plan the roles

Identify independent roles from $ARGUMENTS or the current conversation — only delegate when
the roles are genuinely parallelizable. One role, one worktree, one bee.

The preflight listing is a load-time snapshot. Re-read it before starting bees:

```bash
herdr agent list
```

Read `agent`, `agent_status`, `pane_id`, and `cwd` from the JSON. Drop your own `pane_id`
first — a queen can itself be sitting in a worktree, so path alone does not distinguish you.
Of what remains, a bee is a `claude` agent whose `cwd` is under
`<repo root>/.claude/worktrees/`, where the repo root is `.result.source.repo_root` from
`worktree list`. Every other agent — including any session the user is driving at the repo
root — is off limits: never count it, reuse it, prompt it, or send keys to it.

Cap concurrent bees in this repo at 3, counting bees already `working`/`blocked` toward the
limit. Queue the remaining roles and release one only when a bee's report frees a slot.

### Open a run directory

Bees hand back results as files, so create a run directory outside every worktree and keep the
absolute paths:

```bash
mktemp -d -t herdr-swarm
```

Write a manifest there (`manifest.md`) listing each role: bee name, branch, worktree path,
report path, tab and pane ID, and queue position. Update it as roles are dispatched and reports
arrive — it is what lets you resume after the conversation is compacted.

### Name a bee

Pick one short `<name>` per role matching `[a-z][a-z0-9_-]{0,31}`, unique among live agents,
and reuse it for the worktree label, pane label, and agent name. Derive it from the role
(`parser`, `docs-brew`), not from a branch name — branch names usually contain `/` and are
invalid as agent names. `bee` is the collective noun, never an individual name; only the queen
carries a fixed one.

The branch is `swarm/<name>` unless the user names another prefix or a full branch name.

### Start a bee

Do not use `herdr worktree create`: its `--workspace` / `--cwd` select the *source* repo, not a
destination, and it always spawns a whole new workspace — a swarm of them scatters the UI and
buries the queen. Build the worktree and its pane separately instead.

Resolve the repo root as `.result.source.repo_root` from `herdr worktree list --cwd "$PWD"`,
and give `git` an **absolute** path. A relative one is resolved against the queen's own cwd,
which nests the bee's worktree inside the queen's when the queen is itself in a worktree. Base
the branch on `main` unless the user asks for another base:

```bash
git worktree add -b swarm/<name> <repo_root>/.claude/worktrees/<name> main
```

Now arm the bee's report hook, **before** the agent exists — settings are read at launch, so a
file written afterwards is not picked up. Write `<worktree>/.claude/settings.local.json`:

```json
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "herdr agent prompt <queen> '[swarm:<name>] turn ended — report: <report-path>' >/dev/null 2>&1 || true",
            "timeout": 30
          }
        ]
      }
    ]
  }
}
```

This is what actually wakes you. A bee asked to run a command at the end of its turn may
simply not do it — the hook fires whether or not the bee remembers. `.claude/settings.local.json`
is already ignored by git in this repo, so it never reaches the branch; confirm that before
writing it, and if it is not ignored, tell the bee not to commit it.

Then give the bee a tab **in the queen's own workspace**, so the whole swarm stays in one
place:

```bash
herdr tab create --workspace <queen_workspace_id> --cwd <repo_root>/.claude/worktrees/<name> \
  --label <name> --no-focus
```

Take the pane from `.result.root_pane.pane_id`, label the pane itself (`--label` only names the
tab), and start the agent with a startup timeout large enough for a first launch. Pass no
native agent arguments — the bee runs with its default permission behavior:

```bash
herdr pane rename <pane_id> <name>
herdr agent start <name> --kind claude --pane <pane_id> --timeout 60000
```

A bee asks whether it trusts the files in a directory the first time it starts in a new
worktree path, and herdr may not detect that as `blocked`. A bee sitting on that dialog can
never report back, so check before dispatching:

```bash
herdr agent read <name> --source visible --lines 40
```

**That dialog is the user's to answer, not yours.** If it is present, do not send keys and do
not dispatch the role. Tell the user which bee is waiting, quote what is on screen, and give
them the pane ID so they can answer it themselves. Leave the bee in place and carry on
starting the other roles — you will pick this one up on the next sweep, once the user has
cleared it.

### Dispatch the role

The bee can't see this conversation or any plan file, and its worktree contains only the
repo's tracked files — untracked project instructions never reach it. Every prompt must be
self-contained and state:

- the goal and its definition of done
- the exact paths/files in scope
- project conventions it must follow: write the failing test first and drive the change
  through red → green → refactor; keep comments to a minimum
- whether to commit in its worktree (say so explicitly; it never pushes or opens a PR)
- what it must not do: work outside its own worktree, push, read or write secrets and
  credential files, or start bees of its own
- the reporting contract below, with `<report-path>` filled in

Reporting contract to embed verbatim:

> Before you finish — whether you completed the work or got stuck — write your full report as
> Markdown to `<report-path>`. Its first line must be exactly one of:
>
> ```
> STATUS: DONE — <one-line summary>
> STATUS: FAILED — <why you could not finish>
> STATUS: BLOCKED — <the decision you need from the queen>
> ```
>
> Everything below that line is free-form detail. This file is the only channel back; nothing
> you print in your own final response is read.

The bee never runs a herdr command — the Stop hook does the waking, and the file carries the
substance. Do not ask a bee to notify you itself; that is the instruction it forgets.

Send it without waiting:

```bash
herdr agent prompt <name> "<self-contained prompt>"
```

If this returns `agent_prompt_stalled`, read `--source visible` before doing anything else. If
a dialog is on screen, the bee is waiting on the user — hand it over and leave it. Only if
your prompt text is sitting unsubmitted in the input box with no dialog present, submit it
with `herdr agent send-keys <name> enter`.

### Hand control back

Once every startable role is dispatched, **stop**. Do not run `herdr agent wait`, do not loop
on `herdr agent list`, and do not schedule a timer. Tell the user which bees are flying, which
roles are queued, which bees are waiting on a dialog only they can answer, and that each bee
will wake you when its turn ends. That hook's `herdr agent prompt` lands in your pane as a new
turn — it is the only thing you wait on.

### Handle a report

A turn that starts with `[swarm:<name>] turn ended` is the bee's Stop hook firing. Handle it in
one pass:

1. Read the bee's Markdown report from the path in the message and take its `STATUS:` line.
2. `DONE` — record the outcome in the manifest and free the slot.
   `FAILED` — record why; decide whether to re-dispatch the role, hand it to another bee, or
   surface it to the user.
   `BLOCKED` — answer the question with `herdr agent prompt <name> "<answer>"` and keep the
   slot occupied; the hook fires again when that turn ends.
   No report file, or one with no `STATUS:` line — the bee ended a turn without finishing its
   contract. Read its screen, and either re-prompt it for the report or surface it.
3. If a slot is free and roles are queued, start the next one now.
4. Take one cheap sweep of `herdr agent list` while you are awake. It is the only way to
   notice a bee frozen on a dialog, which by definition cannot report. For any bee that is
   `blocked` or `unknown`, read `--source visible` and **hand it to the user**: name the bee,
   quote the prompt it is showing, and give the pane ID. Never answer it yourself — see the
   send-keys rule below. Treat `unknown` as unresolved, never as done. A bee the user has
   since unblocked shows up ready on this sweep; dispatch its role then.
5. Report the increment to the user, then stop again.

Reading a bee's screen is a fallback, not a source of truth:
`herdr agent read <name> --source recent-unwrapped --lines 120`. If raising `--lines` reveals
no more of a completed response, the agent is on the terminal's alternate screen and those
rows are unrecoverable — re-prompt the bee to write the report file instead.

### Wrap up

When every role has reported, summarize the results per role and list what was created so the
user can clean up: `<name>`, branch, worktree path, tab ID, and the run directory. Cleanup is
`git worktree remove <path>` plus `herdr tab close <tab_id>` — spell both out; the queen does
not run them.

Out of scope: pushing, `gh pr create`, merging to main, worktree cleanup — leave these to the
user or the existing project workflow.

Safety: use `--no-focus` for background work; never run bare `herdr` (it launches or attaches
the TUI); never close panes/tabs/workspaces you didn't create; never run `herdr server stop`
unless explicitly asked.

**Never answer a bee's dialog for it.** Approvals, trust prompts, permission confirmations,
and any other question a bee puts on screen belong to the user. Do not send `enter`, `y`, an
option number, or any other accept — not to unstick a queue, not because the action looks
harmless, not because the same confirmation keeps recurring, and not even when the user has
approved a similar action before. Surface it and stop.

`agent send-keys` therefore has exactly one use here: submitting a prompt of yours that
`agent_prompt_stalled` left sitting unsubmitted in a bee's input box. Send `enter`, only after
a `--source visible` read shows your own prompt text in the box and no dialog on screen, and
only to a bee this run started. If a dialog is on screen, that read has just disqualified the
keystroke — report it instead. Never send a key that would interrupt or exit the agent, and
never send task text as keystrokes; task text always goes through `agent prompt`.

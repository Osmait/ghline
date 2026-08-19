# Agents

Both programs can hand what you are looking at to a coding agent, and both use
the same machinery: [herdr](https://herdr.dev) is asked what is running, and a
message is dispatched to a pane, a new worktree, or the checkout itself.

## What is running

`4` is a fourth tab listing every coding agent [herdr](https://herdr.dev) is
running: what it is, whether it is working, where, and what its title says it
is doing. It re-asks on the heartbeat while you are looking at it, so a state
change shows up without a keypress. This program appears in its own list when
run inside herdr, and says `(this window)` so.

`x` asks where to send it. **What** gets sent is decided by where you are
standing, the same way every other key in this program works:

| Standing in | What travels |
| --- | --- |
| Issues | the issue and its body |
| Pull Requests | the description and the list of changed files |
| Actions | the run, with the selected job's log excerpted |
| Files | that file, up to 600 lines |
| a log | that job or step's log, wherever you came from |
| a diff | that one file's change |

The wording differs with it — "work on this issue" and "diagnose this failing
run" ask for different things, and the first line of a prompt is what an agent
leans on hardest.

A log is the one that needs care. A failing run is routinely tens of thousands
of lines, almost all of it packages resolving, so what travels is the flagged
errors with six lines of lead-in and three of follow-on, merged where they
overlap, capped, and **labelled with what was left out** — `140 of 244 lines
shown`. A truncation the agent cannot see is worse than a shorter excerpt,
because it will reason confidently about a log it thinks it has all of. With
nothing flagged, the tail travels instead, and says so.

## Saying something specific

The templates say the standard thing. When the standard thing is not what you
want said, **type it into the picker**: whatever you write leads the message
and the template follows as context.

```
SEND issue  #87  Investigar implementación de Passkeys…    ↑↓ or ^n/^p · enter
❯ only the parser, ignore the tests
```

Typing goes to that line rather than moving the selection, so the arrows and
`^n`/`^p` do the moving — the same bargain the finder makes, for the same
reason. `esc` closes; `x` is now just a letter. An instruction is not
remembered between questions, because a specific thing is specific.

It goes in front by default, since that is what an agent reads first. A
template that names `{note}` places it wherever you like instead, and one that
does not mention it — which is every default and every config written before
this existed — is unchanged when you type nothing.

The confirmation shows the first fourteen lines of what would be sent and the
total size, so a template that renders badly is caught before an agent reads it.

**Where** it goes is three kinds of destination in one list, because that is one
question:

- **an agent already running** — one call, `herdr agent prompt <pane>`;
- **a new worktree** with claude, codex, opencode or pi — branching `issue-<n>`
  off the checkout and starting the agent in it, isolated from your own work;
- **a new agent in the checkout itself** — no branch, no new files, working on
  whatever is currently checked out and alongside whatever you have open.

The last one is offered after the worktrees, because it is the only destination
that can collide with you, and it is labelled with the branch it would actually
land on — read from `.git/HEAD` rather than assumed to be `main`, since a
checkout sitting on someone's feature branch is common and the difference
matters before you agree to it.

All three go through the confirmation dialog. Merging works this way for the
same reason: it makes something happen outside this program, so it should take
a deliberate `y`.

A destination that cannot take the issue is **listed with the reason** rather
than hidden — knowing every agent is busy beats an empty box. An agent that is
working is refused because typing into it mid-task loses its context; one
stopped on a permission prompt is refused because it would read the task as the
answer; and the window you are reading is refused because sending an issue to
the program showing you the list is legal, useless and confusing.

## Where the repository lives

An agent needs a checkout, and this browses far more repositories than the
machine holds — 143 on GitHub against 20 here, which makes "not cloned" the
normal case rather than an edge one. So the picker has three outcomes, and says
which one it is in: an agent is already there, the repository is cloned and can
be branched from, or it is not on this disk and the answer is a `gh repo clone`
you can copy.

The index is built by walking a few roots and reading each git remote, not by
matching directory names: a clone of `Osmait/sbql` in a folder called
`sbql-experiment` is still that repository, and a folder called `sbql` that is
a fork of someone else's is not. It walks two levels below each root, skips
hidden directories, does not descend into a checkout it has already found, and
stops at a budget rather than hanging on a root pointed somewhere enormous. It
runs once, on the service thread, the first time you ask where something could
go.


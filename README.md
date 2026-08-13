# gh-tui

A GitHub TUI in Rust (`ratatui` + `crossterm`), ported 1:1 from the
`GitHub TUI.dc.html` design in the Claude Design project.

It works against real GitHub through the `gh` CLI: the repos, issues, pull
requests, workflow runs and Actions logs are yours. It also ships the design's
fake data as a demo mode.

## Usage

```sh
cargo run --release            # real data via gh
cargo run --release -- --demo  # the design's fake data
```

Real mode needs a signed-in `gh`:

```sh
gh auth login
```

If `gh` is missing or not signed in, it starts in demo mode and says so on
stderr. The scopes it needs are `repo` (read, merge, close and delete branches)
and `workflow` (Actions logs).

Requires a truecolor terminal. The design's font is JetBrains Mono.

## Where each panel comes from

| Panel | Source |
|---|---|
| Accounts | `gh api user` and `gh api user/orgs` |
| Repos and counters | one GraphQL query with open issues and PRs |
| Issues / PRs / Actions | `gh issue list`, `gh pr list`, `gh run list` |
| Detail | `gh issue view`, `gh pr view` |
| Checks | `gh run view --json jobs` for the run tied to the PR |
| Diff | `gh pr diff`, split per file and per hunk |
| Logs | `gh run view --log`, split per job and step |
| Actions | `gh pr merge/close/reopen` and `DELETE /git/refs` |

The calls run on a separate thread, so the interface never hangs: each panel
shows `loading…` while its response is on the way, and the `gh` error if it
fails. `r` drops the active repo's caches and asks for everything again.

## Navigation

The interface is a grid of panes, walked the way you walk windows in vim:
**`h` and `l` move focus to the neighbouring pane, and `j`/`k` always act on
whichever one has it**. The focused pane is marked with a cyan bar: panes with a
selection put it on the selected row, and panes that only scroll (the body and
the log) put it against the left edge.

| View | Panes, left to right |
|---|---|
| List | Repositories · Issues/PRs/Actions |
| Issue | Repositories · Body |
| PR or run | Repositories · Description · Checks |
| Diff | Changed files · Diff |
| Logs | Jobs and steps · Output |

`enter` drills into the focused pane — repos to the list, list to the detail,
checks to the logs, the tree to the output — and `esc` walks that same path
back. `tab` cycles through the panes, wrapping around; `h`/`l` stop at the ends.

## Keys

| Key | Action |
|---|---|
| `j` `k` | move within the pane |
| `h` `l` | pane left / right |
| `tab` | next pane (cycles) |
| `d` | diff of the PR's files |
| `s` | split / unified diff |
| `w` | ignore whitespace-only changes |
| `g` `G` | start / end of the pane |
| `^d` `^u` | half a page |
| `PgUp` `PgDn` | a whole page |
| `1` `2` `3` | Issues / Pull Requests / Actions |
| `enter` | enter the pane / open |
| `esc` `q` | back one level |
| `a` | switch account |
| `o` | fold / unfold a job (in the logs) |
| `/` | filter (list or log) |
| `e` | jump to the first error in the log |
| `f` | toggle the log's *follow* mode |
| `r` | refresh |
| `:` | command line |
| `?` | help |
| `ctrl-c` | quit |

On a pull request:

| Key | Action |
|---|---|
| `m` | merge (pick the method with `1` `2` `3`) |
| `c` | close, or reopen if already closed |
| `D` | delete the branch |
| `y` / `enter` | confirm |
| `n` / `esc` | cancel |

Commands: `:account`, `:issues`, `:prs`, `:actions`, `:logs`, `:diff`, `:files`,
`:help`, `:q`.

As in the design, `q` and `:q` go back or close the overlay; to quit the program
use `ctrl-c`.

## Files and diff

`d` on a pull request opens the diff view: the changed files with their counts
on the left, the contents on the right. `j`/`k` walks the files, `l` moves to
the diff and `j`/`k` scrolls it there.

- `s` switches between the unified diff (two numbering columns, original and
  new) and the split one (deletions left, additions right).
- `w` ignores whitespace-only changes: `-`/`+` pairs whose contents match once
  whitespace is removed collapse into a single context line.
- A file with no textual changes — a binary, a mode change — shows
  "no textual changes" instead of an empty pane.

From the PR detail, `enter` on the body opens that same view.

## Pull request flow

In real mode the action is executed by `gh` and the list is reloaded from what
GitHub reports; in demo mode the in-memory copy is mutated. The flow is the
same:

1. `m` opens the merge confirmation, which first shows the state of the checks,
   how many approvals there are and whether anyone requested changes.
2. You pick the method — merge commit, squash or rebase — and confirm with
   `enter`.
3. The branch-deletion confirmation follows immediately, like the button GitHub
   offers after a merge.
4. The PR becomes `merged`, the branch is marked with `⊘`, and the repo's
   open-PR count is adjusted in the tab and in the sidebar.

The branch-deletion prompt only appears **if the merge succeeded**, and it
carries the branch name with it rather than re-reading the selection: by the
time the response arrives, the list has already been reloaded. The modal shows
the `owner/repo` you are about to act on.

The rules are GitHub's: a *draft* PR cannot be merged, an already merged PR
cannot be closed, and the branch is only deleted once the PR is resolved. When
an action does not apply, the reason appears in the status bar. Closing a PR can
be undone by reopening it with `c`.

In demo mode everything lives in memory and a restart brings the original data
back. In real mode the changes are real changes on GitHub.

## Layout

| File | Contents |
|---|---|
| `src/theme.rs` | the design's palette and glyphs (`sc()` / `si()`) |
| `src/data.rs` | static data: accounts, repos, issues, PRs, runs, jobs, logs |
| `src/app.rs` | state and reducer, equivalent to the design's `Component` class |
| `src/gh.rs` | invoking `gh` and translating its JSON into the model |
| `src/service.rs` | worker thread: requests and responses over channels |
| `src/actions.rs` | merge / close / reopen / branch deletion, isolated from the UI |
| `src/error.rs` | the error type shared by the `gh` layer |
| `src/ui/` | render per region: header, sidebar, list, detail, diff, logs, status, overlay |
| `src/snapshot.rs` | terminal-free mode for inspecting a render |

## Error handling

The `gh` layer returns a typed `Error` rather than a `String`, so callers can
tell a missing CLI from a repository they cannot read. It implements `Display`
and `std::error::Error`, and carries the subcommand that failed so the message
can name it. At the boundary with the interface each error is reduced to a
one-line summary; failures that look temporary — timeouts, rate limits — add a
suggestion to retry with `r`.

The terminal is owned by a guard that restores it on drop, backed by a panic
hook. A panic therefore leaves the console usable instead of stuck in raw mode
with no echo.

## Tests

```sh
cargo test
```

The suite covers the parsing and layout edge cases that are easy to get wrong
and hard to see: dates with GitHub's zero timestamp, both ANSI escape forms,
unified diffs with renames and no-newline markers, hunk line numbering, wrapping
and truncation at column boundaries (including wide glyphs), scroll clamping,
pane navigation, and the pull request state machine with an empty or filtered
list.

## Terminal-free renders

Useful for comparing against the design or debugging the layout:

```sh
# ANSI dump to stdout: <keys> <width> <height> <ticks>
cargo run -- --snapshot "3<enter><enter>" 150 40 6

# the same render as SVG
cargo run -- --svg "" 150 40 > list.svg

# with real data, waiting on gh between keys
cargo run -- --svg-live "<enter><enter>" 150 40 > logs.svg
```

Keys are written literally, with `<enter>`, `<esc>`, `<tab>`, `<bs>` and the
arrows in angle brackets.

## Differences from the design

- Merge, close and branch deletion are not in the design: they are an addition,
  with their own confirmation modal. Since the design uses `d` for the diff,
  branch deletion — which is destructive — lives on `D`.
- The design leaves `w` (ignore whitespace) as a label with no effect; here it
  really does filter the pairs that differ only in whitespace.
- The design is also mouse-driven, and its `h`/`l` only toggled between the
  sidebar and the content. Here `h`/`l` walks every pane in the view and `j`/`k`
  acts on the focused one, so a PR description or a log's output is read with
  the same two keys as everything else. When there is more content than fits, a
  scrollbar appears at the edge of the pane.
- 1 px borders are drawn as a row/column of characters, so each panel takes one
  more line than in the mockup.
- Issue/PR labels, which are bordered boxes in HTML, are drawn `[like this]`.
- The visibility column of the repo pane is empty: the design defines its colour
  (yellow when private) but the glyph was lost when the HTML was saved. The
  markers live in `theme::PRIVATE_MARK` / `theme::PUBLIC_MARK`.
- Below 90 columns the repository pane is hidden, and below 40x8 a notice is
  shown instead of the interface.
- With real data the checks pane shows the actual workflow and run instead of
  the `CI #1841` and the billing line the design had hard-coded.
- The account picker lists your user and your organisations on github.com; the
  design also included a GHE host, which is not covered here.

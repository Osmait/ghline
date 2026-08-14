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

The calls run on a separate thread, so the interface never hangs. While a
response is on its way each panel draws the outline of what is coming —
placeholder rows in the proportions the real ones will have, with a highlight
band travelling down them — rather than a word in an empty box, so nothing
jumps when the data lands. A failure shows the `gh` error instead. `r` drops
the active repo's caches and asks for everything again.

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

The repository pane starts hidden: with sixty repositories it is a wall, and
`[`/`]` step through them without it while the finder reaches any of them by
name. `b` brings it back and gives its width to the content. It stays
hidden until you ask for it back, and the logs and diff views never show it —
the design gives them the full width. Below 90 columns there is not enough room
for it whatever you asked for, and in every one of those cases `h` will not walk
to it, because a pane that is not on screen is not a pane.

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
| `t` | switch theme |
| `b` `^b` | hide / show the repository pane |
| `[` `]` | previous / next repository |
| `p` `^p` | the finder |
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
`:theme`, `:sidebar`, `:find`, `:help`, `:q`.

As in the design, `q` and `:q` go back or close the overlay; to quit the program
use `ctrl-c`.

## The finder

`p` — or `^p` — opens it over everything else. `tab` walks the four sources and
keeps what you typed, since switching usually means "the same words, somewhere
else". `↑`/`↓` or `^n`/`^p` move, `enter` goes there, `esc` closes.

| Source | Where it looks |
|---|---|
| repos | the repositories already loaded, filtered as you type |
| issues | `gh search issues` across your repositories |
| pull requests | `gh search prs` across your repositories |
| commits | `gh search commits` across your repositories |

Repositories are already in memory, so they filter with no latency and the
matched letters are highlighted. The other three go to GitHub once typing
pauses, and are shown in the order the server ranked them.

Commits are the exception worth knowing about: GitHub rejects a commit search
made of qualifiers alone, so that source has nothing to show until you type
something. The other three list without a query.

Matching is a small scorer in `src/fuzzy.rs` rather than a dependency — letters
that run together beat letters scattered about, the start of a word beats the
middle, and a shorter name beats a longer one. It matches greedily, which is
documented where it could otherwise look like a bug.

## Themes

`t` opens the picker. It applies as you move through it, so what you are judging
is the interface itself rather than a name in a list — `enter` keeps the one you
land on, `esc` puts back the one that was on when you opened it.

Two ship for now: the design's own palette, and Catppuccin Mocha. A theme is a
whole `Palette` and switching one in is a single store, so the change lands on
the very next frame. Adding a third is a `Palette` literal and one line in
`Theme::ALL`; a test walks every theme and fails if a role is left undefined,
which would otherwise show up as an invisible pane.

Mocha is mapped by the role each colour plays rather than by name: the design
keeps its panels a shade *lighter* than the background, so mantle is the ground
and base is the panel, not the other way round.

## Markdown

Bodies are Markdown, and they are rendered as such: headings lose their hashes
and gain weight, emphasis becomes emphasis, inline code and links take the
design's palette, and tables keep their shape. `tui-markdown` does the parsing,
taken without its default feature so that syntax colouring inside fences — and
the C dependency it brings — stays out of the build.

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
| `src/data.rs` | the model: items, statuses, diffs — no dependencies of its own |
| `src/demo.rs` | the design's fixture, apart from the model it fills in |
| `src/app/` | the state: `mod` what it is, `select` what it answers, `load` what it fetches, `input` how it reacts |
| `src/gh.rs` | invoking `gh` and translating its JSON into the model |
| `src/service.rs` | worker thread: requests and responses over channels |
| `src/actions.rs` | merge / close / reopen / branch deletion, isolated from the UI |
| `src/error.rs` | the error type shared by the `gh` layer |
| `src/ui/` | render per region: header, sidebar, list, detail, diff, logs, status, overlay |
| `src/ui/markdown.rs` | Markdown bodies, folded and mapped onto the design's palette |
| `src/snapshot.rs` | terminal-free mode for inspecting a render |

## Layering

Each module only reaches downwards, which the import graph makes easy to
check:

```
data, error        model and failures, no dependencies of their own
theme, gh          presentation and infrastructure, both read the model
demo               the design's fixture
service            blocking gh calls on a worker thread, over channels
app                state and reducer
ui                 render per region; reads the state, mutates nothing
```

The model carries no presentation: a label travels as RGB and a review as a
`ReviewState`, and `theme` is what turns either into a terminal colour. No
module under `ui/` imports `gh` or `service`, so the render can never reach the
network.

States — open, draft, merged, success, failure and the rest — are a `Status`
enum rather than strings. `theme::state_color` and `state_icon` match on it
exhaustively, so a new state makes the compiler ask what it should look like.

An item's kind-specific fields live in the variant they belong to rather than
side by side in one struct, so an issue cannot be given check results and a run
cannot be asked for a branch to delete:

```
Item { num, title, state, author, when, body, labels, detail }
  detail: Issue { comments, comment_list }
        | Pr    { checks, add, del, files, branch, reviews, file_list, … }
        | Run   { event, workflow, dur }
```

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
cargo test          # 86 unit tests
cargo clippy        # the lint set configured in Cargo.toml
cargo machete       # unused dependencies
cargo audit         # advisories against the dependency tree
```

The lint set in `Cargo.toml` forbids `unsafe`, and warns on `unwrap`, `expect`,
`panic`, `todo`, `dbg!` and `print!` outside the modules that legitimately need
them. It is kept deliberately narrow: the full `pedantic` group mostly objects
to the `as u16` casts a cell-grid renderer is made of.

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

# any view held in its loading state, to look at the skeletons
cargo run -- --svg-loading "" 150 40 5 > loading.svg
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

# gh-tui

A GitHub TUI in Rust (`ratatui` + `crossterm`), ported 1:1 from the
`GitHub TUI.dc.html` design in the Claude Design project.

It works against real GitHub through the `gh` CLI: the repos, issues, pull
requests, workflow runs and Actions logs are yours. It also ships the design's
fake data as a demo mode.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/Osmait/github-tui/main/install.sh | sh
```

Downloads the release binary for your machine, checks it against the published
SHA-256, and puts it in `~/.local/bin`. No toolchain, no root, and nothing
outside the install directory is touched. Apple Silicon Macs and x86_64/aarch64
Linux are covered; anywhere else builds from source, which is a `make install`
away.

Piping a script from the internet into a shell is worth being wary of, so read
it first if you would rather:

```sh
curl -fsSL https://raw.githubusercontent.com/Osmait/github-tui/main/install.sh -o install.sh
less install.sh
sh install.sh
```

To choose the location or pin a version:

```sh
GITHUB_TUI_INSTALL_DIR=/usr/local/bin GITHUB_TUI_VERSION=v0.1.0 sh install.sh
```

You also need the [`gh` CLI](https://cli.github.com), signed in — that is how
this reads GitHub. Without it the demo mode still runs.

### From source

With a Rust toolchain, from a clone:

```sh
make install
```

That is `cargo install --path . --locked`: an optimised build put in
`~/.cargo/bin`, which the Rust installer already adds to your PATH. `make
uninstall` takes it off again.

`make` on its own lists the rest — `build`, `run`, `demo`, `test`, `lint`,
`check`. They are thin wrappers over cargo, and `make check` runs exactly what
CI runs.

## Usage

```sh
github-tui         # real data via gh
github-tui --demo  # the design's fake data
```

Or without installing, from a clone:

```sh
make run           # cargo run --release
make demo          # cargo run --release -- --demo
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

## All repositories

A session opens on **all repositories** rather than on one of them: with a
hundred repos, "what is going on" is a better first question than "in which
one". It is the first row of the repository pane, and `[` steps off it onto a
single repository whenever you want the narrower view.

Issues and pull requests are one GraphQL search each, so gathering them costs
no more than opening a single repository would — and the rows lose nothing:
the diff counts, the file count and the branch are all in the same answer.
Every row says which repository it came from, and `/` filters on that as well
as on the title, so `/sbql` narrows a mixed list back down to one project.

Actions cannot be gathered the same way, because GitHub has no cross-repository
Actions API — it really is one call per repository. Two things keep that
honest: the repository query already asks which repos have a
`.github/workflows` directory at all, so only those are called (twenty of a
hundred and forty-three, here), and the calls go out sixteen at a time. It
lands in about a second and a half.

What a row belongs to and what pane it is listed in are no longer the same
thing, so everything downstream of the selection — the body, the diff, the
checks, the logs, and merging or closing it — follows the item's own
repository. That matters more than it sounds: a gathered list really does hold
two different `#14`s, and both the request and the answer have to name the
repository or one pull request's body ends up on the other.

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

## Files

`4` is the repository's file tree, read straight off GitHub — no clone needed.
A tree on the left, the file on the right, the same two-pane shape as the logs
and the diff. Directories start closed; `enter` or `o` opens one, `enter` on a
file moves to its contents, and `x` sends the file to an agent.

The whole tree comes down in one request against `HEAD`, so nothing has to know
what the default branch is called and opening a directory costs nothing. Only
the file you actually select is fetched, and only if it is under half a
megabyte — past that the pane says how big it is rather than stalling on it. A
file that turns out to be binary says so instead of painting the pane with
replacement characters, and a tree GitHub truncated says that too, because a
partial listing that stays quiet reads as a smaller repository.

`/` finds a file without walking to it: the filter flattens the tree, matches on
the whole path rather than the name, and leaves the directories out, since a
directory is not something you can read or send.

Contents are wrapped rather than cut. A long line in a config file is still
worth reading, and there is no horizontal scroll here.

## Agents

`4` is a fourth tab listing every coding agent [herdr](https://herdr.dev) is
running: what it is, whether it is working, where, and what its title says it
is doing. It re-asks on the heartbeat while you are looking at it, so a state
change shows up without a keypress. This program appears in its own list when
run inside herdr, and says `(this window)` so.

`x` asks where to send it. **What** gets sent is decided by where you are
standing, the same way every other key in this program works:

| Standing in | What travels |
|---|---|
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

### Where the repository lives

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

### Settings

Eight keys in `~/.config/github-tui/config`, all optional:

```
prompt      = Work on {repo}#{num}: {title}\n\n{url}\n\n---\n\n{context}
prompt-pr   = Review {repo}#{num}: {title}\n\n{url}\n\n{context}
prompt-run  = Diagnose this failing run in {repo}.\n\n{title}\n{url}\n\n{context}
prompt-diff = Explain this change from {repo}#{num}\n\n{url}\n\n{context}
prompt-file = Here is a file from {repo}.\n\n{url}\n\n---\n\n{context}
agents      = claude, codex, opencode, pi
agent-icons = claude=✳, codex=◆, opencode=◇, pi=π
clone-roots = ~/orca, ~/Projects
```

One template per kind of subject. The placeholders are `{repo} {num} {title}
{url} {context}`, where `{context}` is the body, the file list or the log
excerpt depending on what is being sent; an unknown one is left as itself
rather than blanked, so a typo looks like a typo. `\n` is two characters in a
config file and becomes a real newline on the way out. The URL is in every
default because an agent that can read the thing asks fewer questions.

`agent-icons` overrides the mark drawn beside each agent, as
`claude=✳, codex=⌬`. Only two of the defaults are real marks: `π` is what pi
puts in its own terminal title and `✳` is what Claude Code prints for itself.
Codex and opencode have no glyph of their own and get a neutral one, because an
invented brand icon is decoration pretending to be information.

They are plain BMP symbols rather than Nerd Font glyphs on purpose. A Nerd Font
icon lives in the private use area, where `unicode-width` has to guess it is one
column while the non-Mono font variants draw it across two — which is how a
column chart quietly goes crooked. If your font can do better, say so here; an
entry that is not a single character is ignored rather than drawn.

`agents` is what to offer for a new worktree — herdr decides what it can
actually start, so an unsupported name comes back as herdr's own refusal
rather than a guess at one.

If the agent then fails to start, whatever was just made is undone: a worktree
is removed, a workspace is closed. A half-built one would be worse than the
failure — the next dispatch would collide with a branch that already exists,
and you would have a window you never saw appear. Undoing never touches your
own checkout either way.

## The mouse

It is there if you want it, and it adds nothing the keyboard cannot already do.
A click is `h`/`l` then `j`/`k`: it focuses the pane it landed on and selects
the row under the pointer. A double click is `enter`. The wheel is `j`/`k`, on
whatever the pointer is over — reaching for a pane to read it is not a decision
to work in it, so scrolling never takes the focus. Tabs are clickable, and so
are the rows of the finder, the theme picker and the account switcher; clicking
away from one closes it, on the same terms as `esc`.

A confirmation ignores the mouse entirely. "Merge this?" wants a deliberate
answer, and a stray click is not one.

Only the renderer knows where anything ended up, so each pane records the
rectangle it drew and enough to turn a row back into an index (`src/app/hit.rs`).
They are rebuilt every frame and read newest-first, which is what puts a modal
in front of the panes it covers without either having to know about the other. A
pane the renderer forgets to register is a pane the mouse cannot reach and
nothing else would notice, so a test walks every pane of every view and fails if
one is missing.

Capturing the mouse takes the terminal's own click-to-select with it. Most
terminals still select with `shift` held down; `--no-mouse` turns the whole
thing off if you would rather have it back.

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

The one you keep is remembered: `enter` writes it to
`~/.config/github-tui/config` (or `$XDG_CONFIG_HOME`), and the next start reads
it back. The file is `key = value` lines, safe to edit by hand; keys it does not
recognise are left alone rather than dropped, so a config written by a newer
version survives an older one. A theme that cannot be written is still applied,
and says so — silently forgetting looks like a bug. The headless render modes
deliberately ignore it, so a snapshot is the same frame on any machine.

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

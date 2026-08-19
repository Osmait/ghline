# Development

```sh
make            # lists every target
make check      # exactly what CI runs — the gate before a change is done
make install    # cargo install --path . --locked
```

## Layout

| Path | Contents |
| --- | --- |
| `crates/ghline-app/` | GitHub model, `gh` source, state and views |
| `crates/diffline-app/` | diff model, VCS source, review state and views |
| `crates/line-shared/` | configuration, clone discovery, logging and worker contracts |
| `crates/tui-kit/` | terminal runtime, drawing primitives, themes and input |
| `crates/source-text/` | syntax highlighting, wrapping and terminal-safe text |
| `crates/agent-mux/` | agent discovery and dispatch through multiplexers |
| `src/bin/ghline/` | process setup and terminal adapter for `ghline` |
| `src/bin/diffline/` | process setup and terminal adapter for `diffline` |

## Layering

Each application follows the same one-way stack:

```
view → state → source → data/model
              │
              └── blocking gh/git work stays on a worker thread
```

`ghline-app` and `diffline-app` never depend on each other. Both sit above
`line-shared` and the reusable workspace crates, so imports name the owner
clearly: `ghline_app::state`, `diffline_app::model`, `line_shared::config` and
`tui_kit::theme`.

The model carries no presentation: a label travels as RGB and a review as a
`ReviewState`, and the theme turns either into a terminal colour. No view
imports a process source, so rendering can never reach GitHub or Git.

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

The golden frames and most of the state tests are written against
`ghline_app::source::fixture`: two accounts, seven repositories and rows built from
a table. It used to be nine hundred lines behind a cargo
feature, so a released binary would not carry it; at a page it is cheaper to
compile in than to gate, and `App::new` no longer has to ask which kind of data
it is holding.

```sh
cargo test          # 678 tests across the workspace
cargo clippy        # the lint set configured in Cargo.toml
cargo doc           # the rustdoc lints, which are denied
make cov            # what fraction of the crate those tests execute
make audit          # dependencies, advisories, licences, sources, spelling
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

Those tests say what a view is *about* — a count in a tab, a line that stops at
the pane's edge — which is why they survive a redesign of everything around the
thing they name, and also why the layout can move underneath them. `tests/`
takes the other half: eighteen whole frames of the fixture, compared character for
character, so any change to any of them fails with the screen before and after
as the diff.

```sh
cargo test --test frames        # the golden frames
cargo insta review              # look at what changed, accept or reject
INSTA_UPDATE=always cargo test --test frames   # accept without cargo-insta
```

A golden is only worth what the look at it was worth. Accepting a frame you
have not read turns a failing test into a passing one and nothing else.

Both of those are examples: inputs somebody thought of. `tests/props.rs` is the
other kind. Three things here parse input nobody in this repository wrote — the
output of `git diff`, a line of source on its way to being coloured, a line on
its way into cells — and what it asserts about them is what has to hold for
every input rather than for a chosen one: that the pieces a line is wrapped
into still spell the line, that an offset handed back can slice the string it
came from, that a row's numbers only go forwards.

```sh
cargo test --test props                  # a few hundred generated cases each
PROPTEST_CASES=10000 cargo test --test props    # a longer look
```

It earned its place on the first run, with two bugs the examples had missed.
Colouring a line hung — forever, inside the draw, with no key that interrupts
it — on any word starting with a letter that is not ASCII: `año`, `café`,
`Ünicode`, `漢字`. The loop was entered on `is_alphabetic`, true of every letter
there is, and left on `is_ascii_alphanumeric`, false of most of them. And a
hunk header saying `@@ -4294967295 @@` overflowed the line counter two rows
later, which is a panic in a debug build. Both are fixed, and both now have an
ordinary named test next to the code as well.

When a property fails, the input is shrunk to the smallest one that still fails
and the seed is written to `tests/props.proptest-regressions`, which is
committed — so the case is re-run first, for ever, by everyone.

`make cov` says how much of the crate all of that actually executes — 72.7% of
lines today, which CI holds a floor under rather than a target over. The total
is the least useful line of it. The report is per file, and read that way it
says where the tests are and are not: `crates/tui-kit/` sits above 97% and the drawing
code for both programs between 70% and 95%, while `crates/ghline-app/src/source/gh.rs` — the
JSON coming back from `gh`, parsed field by field — has seven hundred lines
nothing has ever run, and `view/ui/confirm.rs` and `view/ui/dispatch.rs` have
none at all. The two binaries and `crates/tui-kit/src/run.rs` are low for a different reason:
they are the terminal and the event loop, and what tests them is running them.

## What a frame costs

```sh
cargo bench             # all four
cargo bench -- draw     # one
```

| | on one desk, for scale |
| --- | --- |
| `draw` — a whole frame at 160×44 | ~145 µs |
| `highlight` — the lexer over 600 lines | ~96 µs |
| `rank` — the finder over 500 repositories | ~57 µs |
| `wrap_ranges` — one long line | ~410 ns |

The loop never waits longer than 16 ms for a keystroke, so a frame has about
a hundred times its own cost in hand. The numbers are here to be compared
against themselves after a change, not against another machine — which is also
why CI does not run them: a shared runner varies by more than the differences
worth catching.

## Terminal-free renders

Useful for reviewing the layout, or for attaching a frame to a bug report.
The workspace has two binaries and no `default-run`, so `--bin` is not
optional:

```sh
# with real data, waiting on gh between keys — in every build
cargo run --bin ghline -- --svg-live "<enter><enter>" 150 40 > logs.svg

# diffline draws whatever repository you point it at
cargo run --bin diffline -- . --svg "j" 150 40 > diff.svg

# the three that draw the fixture rather than GitHub
cargo run --bin ghline -- --snapshot "3<enter><enter>" 150 40 6
cargo run --bin ghline -- --svg "" 150 40 > list.svg
cargo run --bin ghline -- --svg-loading "" 150 40 5 > loading.svg
```

Keys are written literally, with `<enter>`, `<esc>`, `<tab>`, `<bs>`, `<del>`,
`<home>`, `<end>`, `<pgup>`, `<pgdn>` and the arrows in angle brackets. A
modifier goes inside them — `<c-c>` is control-c, `<a-x>` is alt-x, `<c-up>` is
control-up — and `<lt>` and `<gt>` are the two characters the notation is
otherwise made of.

### The screenshots in the documentation

`docs/img/` is generated by this, not captured by hand, so a frame in the
documentation cannot show an interface that no longer exists. To redraw them
all after a change:

```sh
cargo build --release
./target/release/ghline --svg "" 150 30 > docs/img/ghline-list.svg
./target/release/ghline --svg "2<enter>" 150 34 > docs/img/ghline-pr.svg
./target/release/diffline . --svg "jjjV jj nthis names keys that are not bound any more<enter>" 150 30 > docs/img/diffline.svg
```

The two ghline frames draw the fixture rather than anybody's GitHub, which is
what keeps a private repository name out of a public README. diffline's draws
this repository, so what it shows is whatever the working tree held when it was
taken.

## Recording a session

Neither program can print anything while it runs: the screen is theirs, and a
`println!` lands in the middle of a frame and is gone at the next redraw. So a
bug report has been whatever the reader remembered doing.

```sh
diffline --log run.log .
ghline --log run.log
```

Every keystroke, click and error goes into the file, stamped with the
milliseconds since it opened — and the last line is the command that plays the
session back:

```
+      0ms diffline 0.1.0 — unix 1786839021
+      0ms repo /home/you/project
+    412ms key j
+    588ms key /
+   1104ms mouse down Left at 12,4
+   3320ms replay: diffline --svg "j/" 160 44
```

Which is the point of it. What is recorded is written in the notation the
headless renders read, so a report is not a description of the bug — paste the
last line and the frame is in front of you. A round-trip test keeps the two
honest with each other.

Each program names the flag it actually has, and both of them work in a
released binary: `--svg` draws the repository diffline was pointed at, and
`--svg-live` replays the keys against real GitHub. Neither touches the
fixture, which is why neither went behind the feature with it.


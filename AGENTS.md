# Working in this repository

Two terminal programs over one set of parts: `ghline` browses GitHub
through the `gh` CLI, `diffline` reviews the diff in front of you and hands
notes to a coding agent.

**Read [CODE-STYLE.md](CODE-STYLE.md) before writing Rust here.** It is the
rulebook, and this file is the map. Each application crate's `lib.rs` explains
its architecture; the README is the user-facing tour.

## Where things are

```
crates/ghline-app/   GitHub: data → source → state → view
crates/diffline-app/ diffs:  model → source → state → view
crates/line-shared/  application glue — config, clones, logging and workers
crates/*/            reusable boundaries — parser, text, matching, errors,
                     agent multiplexers and the TUI toolkit
src/bin/             thin process adapters for `ghline` and `diffline`
tests/               golden frames (insta), properties (proptest)
benches/             divan, what each layer costs
scripts/             the one job the Makefile could not say in five lines
```

Within a program every arrow points down and none point back up. The two
application crates never depend on each other. `line-shared` may compose the
reusable crates, but Cargo prevents it from naming either application.

## Commands

```
make check     # everything CI checks — fmt, clippy, doc, tests
make test      # cargo test
make lint      # fmt --check, clippy -D warnings, cargo doc
make audit     # machete, deny, typos, zizmor
make bench     # divan, grouped by module
make bench-cmp # the same ones either side of your changes, with a noise check
make flame     # perf + inferno, where the time inside one goes
```

`make check` is the gate. Run it before saying a change is done; do not
report a change as working on the strength of it compiling.

## The rules that are not negotiable

The rest is in CODE-STYLE.md, but these are the ones that get broken by
someone moving fast:

1. **No `unwrap`, `expect`, `panic!` or `todo!`** outside tests. They are
   lints. Invariants are `debug_assert!`. Values that came from `gh`, `git`, a
   config file or a terminal event are never indexed without a check.
2. **Errors keep their type.** `line_shared::error::Error` for something we ran
   saying no, `Failure` for this program declining. Never `Box<dyn Error>`,
   never a `String`, never a flattened cause chain.
3. **Comments say why, not what.** The signature already says what. A comment
   earns its place by recording the alternative that was tried or the bug that
   motivated the shape. Every file opens with `//!`, without exception.
4. **Behaviour gets a test, and the test name is a sentence.**
   `scroll_into_view_never_scrolls_past_the_end`, not `test_scroll`. Prefer
   the lowest level that can fail.
5. **Do not add a dependency** without saying in the commit message what it is
   worth.
6. **Do not widen the lint set casually, or narrow it at all.** `pedantic` was
   tried and rejected for reasons written down in `Cargo.toml`.

## Snapshots

`insta` golden frames pin what a screen looks like. If one fails, read the
diff before accepting it — a snapshot accepted without reading is the entire
failure mode of snapshot testing. `cargo insta review`, not
`cargo insta accept`.

## Commit messages

Lowercase, declarative, describing what is now true rather than what was done:

```
the tests are measured, the lower bounds are checked, the tarballs are signed
ci holds a read-only token, and two linters now say so
an agent row is one component now, and it has tests
```

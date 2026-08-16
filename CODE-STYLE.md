# Code style

What this crate does, and why, so that the next change looks like the last
one. Most of it is already true of the code — writing it down is what stops
it drifting when somebody new, or something new, edits a file at three in the
morning.

The README explains the two programs to whoever runs them. This explains them
to whoever changes them. Where the two overlap — layering, error handling,
tests — the README is the tour and this is the rule.

Nothing here is invented for this repository. Where a convention comes from
somewhere with more mileage on it, it says so at the bottom.

---

## The compiler goes first

Everything that can be a lint is a lint. A rule in a document is checked by
whoever remembers it; a rule in `Cargo.toml` is checked on every save.

The lint set lives in `[lints]` in `Cargo.toml` and every entry there has a
comment saying what it caught. Adding one is a normal change. Removing one
needs a reason in the commit message, not just in the diff.

The set is deliberately narrow. `clippy::pedantic` was tried and rejected: in
a program that is a grid of cells, most of what it finds is `as u16`, and a
lint you learn to skim is a lint that stops working. Six lints were kept from
that experiment because they had real hits — they are listed by name rather
than by group, for the same reason.

`rust-toolchain.toml` pins an exact compiler. That is what makes a lint
difference between two machines a finding rather than a version gap, and it is
why CI carries a second, advisory `stable` job: pinning means the day clippy
changes under us should be news, not a surprise six months later.

## Panics

There is no `unwrap`, no `expect`, no bare `panic!`, and no `todo!`. All four
are lints. Tests opt out once, in `clippy.toml`, rather than twenty-three
times above twenty-three `mod tests`.

Invariants are `debug_assert!`, not `assert!`. This is a choice and not the
default one: a violated invariant here costs a wrong cell, and stopping costs
a terminal left in raw mode with a queue of unsent comments in it. Write the
assertion to say what the function is *for*, so the message reads as a
sentence when it fires:

```rust
debug_assert!(
    sel >= len || (sel >= *offset && sel < *offset + height),
    "selection {sel} is outside the window {}..{} of {len}",
    *offset,
    *offset + height
);
```

**Indexing is the exception, and it is a real one.** `buf[i]` and `s[a..b]`
panic exactly like `unwrap` does, and there are around 255 of them. In a cell
renderer, indexing is the idiom — `clippy::indexing_slicing` would be 255
warnings on the first run and zero attention on the second. So the policy is
not "nothing panics", it is: *no panic on a value that came from outside the
process.* Anything derived from `gh` output, `git` output, a config file or a
terminal event goes through `get`, `get_mut` or a checked slice. Anything
indexed by a number this crate computed itself may be indexed, and if the
index is non-obvious it gets a `debug_assert!` above it.

Arithmetic that could go under zero uses `saturating_sub`. Arithmetic that
could leave `u16` is widened to `u32` and narrowed back once, at the end:

```rust
pub fn pct(avail: u16, p: u16) -> u16 {
    (u32::from(avail) * u32::from(p) / 100) as u16
}
```

## Errors

Two types, and the difference between them is load bearing.

`shared::error::Error` is something we ran saying no: a program that would not
spawn, a non-zero exit, output that was not the JSON we asked for, a field
that was not there. Every case names which of `gh`, `git` or `herdr` it was,
because a type shared by three callers that assumes one of them will tell
somebody to install the wrong thing.

`shared::error::Failure` is wider: `Ran(Error)` for the above, `Refused(String)`
for this program declining — a file too large to open, a worker thread that is
gone. Inventing a `Spawn` error for a dead thread reads as a lie the first
time somebody prints the cause.

The rules that come out of that:

- **No `Box<dyn Error>` and no `String` as an error type.** A missing program
  is not fixed the same way as a repository you cannot read, and the caller
  has to be able to tell them apart.
- **The cause chain survives.** `source()` returns the `io::Error` or the
  `serde_json::Error`, it does not flatten it into a message. There is a test
  pinning this.
- **Two renderings, on purpose.** `Display` is the full sentence; `brief()` is
  the one line a status bar has room for. A view calls `brief()`. Nothing
  calls `brief()` and then adds context to it.
- **`is_transient()` is the retry decision, and it lives on the type.** Not in
  the view, not in the service. A decision is never transient — it will be
  made the same way next time.
- A new variant is added to the enum rather than smuggled into an existing
  one's `String`.

## Naming

Rust's conventions, without local dialect: RFC 430 casing, and `as_` /
`to_` / `into_` meaning borrowed / expensive / owning, in that order. Getters
are `name()`, not `get_name()`.

Word order is consistent across the crate: `<verb>_<subject>` for actions
(`scroll_into_view`, `expand_tabs`), `is_<predicate>` for questions
(`is_open`, `is_settled`, `is_transient`). If a new function does not fit one
of those shapes, it is usually two functions.

Files are named for what they are, and the path is what tells you which
program you are in. Each program has a `service`, a `view` and a `hit`; the
directory disambiguates them, so the file does not need to.

## Types

**Derive the common traits eagerly.** `Debug` on anything public, `Clone`
where it makes sense, `Copy` on the small enums and the coordinate-shaped
structs, `PartialEq`/`Eq` on anything a test will compare, `Hash` on anything
that will be a map key. `Debug` in particular is not optional: a type without
it is a type that cannot appear in an `assert_eq!` failure.

**A field-per-case enum beats a bag of `bool`s.** `Status` is one enum
covering issues, pull requests and CI because `Item::state` genuinely holds
any of them; two enums would mean a conversion at every use site.

**`&str` in, `String` out.** Parameters take `&str` unless the function is
storing the value; `needless_pass_by_value` is on and will say so.

**Pure functions get `#[must_use]`.** If a function's whole effect is its
return value then ignoring that value is a bug, and it is the one bug class
still available in a crate where nothing panics. Do not apply it to anything
that also mutates — `scroll_into_view` writes through `&mut usize` and does
not get it. `tui::geom`, `shared::fuzzy`, `shared::text` and `shared::ago`
carry it today; the `is_*` predicates on `Status`, `Error` and `Failure` are
the obvious next ones.

**Reach for a newtype when two parameters of the same type are adjacent.**
`fn f(x: usize, y: usize)` is a function whose arguments can be swapped
silently. This crate has very few of these today and should not grow more; the
existing ones (`scroll_into_view`, `inset`) are grandfathered and documented
rather than converted.

## Modules and visibility

One arrow, and it points down. `view` reads `state`, `state` asks `source`,
`source` knows `data`, and `data` knows none of them. The two programs never
name each other. Nothing in `shared` may name either program — that one is
enforced by a test in `src/shared/mod.rs`, because it has been broken three
times.

`pub` means "part of this crate's surface", not "reachable from the file next
door". Inside a private module, use `pub(crate)` — `pub` there is a claim the
module cannot back. This is `unreachable_pub` in `Cargo.toml`, so it is the
compiler's rule and not this document's: the twenty-four items that had
drifted are `pub(crate)` now, and what is still `pub` is the API.

`tui` and `shared` sit side by side rather than stacked, and `lib.rs` says so.
The one edge in each direction is drawn in the diagram precisely because it is
the place the rule does not hold.

## Documentation

Every file opens with `//!` saying what is in it and why it is a separate
file. Not one file in the crate is missing this, and that is a property worth
keeping — it is also the cheapest thing here to break.

Item docs follow [RFC 1574]: a one-line summary, a blank `///` line, then the
rest in complete sentences. Function summaries are third person — "Keeps `sel`
visible", not "Keep `sel` visible".

The house rule on top of that is: **say why, not what.** The signature already
says what. A comment earns its place by recording the alternative that was
tried, the bug that motivated the shape, or the constraint that is not visible
from here.

```rust
/// Centred with a gutter kept either side, so the thing underneath still
/// shows and the modal reads as floating over it rather than replacing it.
///
/// The difference from `centered` used to be the difference between the two
/// programs' copies of this function, which is to say it was an accident.
```

Rustdoc lints are `deny` in `Cargo.toml` and CI runs `cargo doc`, because a
broken intra-doc link is a comment that lies silently.

**Examples in docs are tests.** For a pure function, a short ```rust fence is
documentation and a test at once, which is why CI has a `cargo test --doc`
step. There are eight, all in the pure layer, and they set the pattern: each
shows the case that is easy to get wrong rather than the case that is obvious.
`fuzzy::score` demonstrates that the match is greedy — `"gt"` against
`"github-tui"` takes the `t` inside "github" — because that is the behaviour
somebody will otherwise assume away. An example asserting `pct(100, 50) == 50`
would have been worth nothing.

`missing_docs` is on for `shared` and `tui`, as an inner attribute on each of
those two modules rather than crate-wide. Both are toolkits with two
consumers, and a toolkit is documented or it is guessed at; `github` and
`diffline` are read by whoever is changing them, which is a different job.

## Formatting and imports

`cargo fmt` decides, and `rustfmt.toml` pins the settings so that two machines
cannot disagree — the same reason the toolchain is pinned. Do not hand-format
around it.

Imports are grouped std / external / crate, blank line between, which is what
the existing files do by hand.

## Tests

There are around 617 of them. The conventions that got there:

- **A test name is a sentence about behaviour**, not about the function:
  `scroll_into_view_never_scrolls_past_the_end`,
  `a_missing_program_names_the_one_that_is_missing`. Read the list of names
  and you have read the spec.
- **The comment above a test says which bug it is.** "It used to tell
  everybody to install the GitHub CLI, including when what was missing was
  git." That is what stops the test being deleted as redundant later.
- **Prefer the lowest level that can fail.** A unit test on the parser beats a
  golden frame; a golden frame beats a test that drives the loop. Add the
  higher-level one only to check the wiring, not the logic.
- **Golden frames** (`insta`, `tests/frames.rs`) pin what a screen looks like.
  Review the snapshot diff — accepting one without reading it is the whole
  failure mode of snapshot testing.
- **Properties** (`proptest`, `tests/props.rs`) go on anything parsing input
  nobody here wrote: diff output, source lines, text about to be cut into
  cells. `tests/props.proptest-regressions` is committed, so a shrunk failure
  becomes a permanent example.
- **Benchmarks** (`divan`, `benches/frame.rs`) exist so that a claim about
  cost is measured. Do not optimise on the strength of reading the code.
- **Architectural rules get a test.** The `shared` boundary is checked by
  reading the directory. That is unusual and it is correct: the rule was
  broken three times by people who had read the comment.

## Dependencies

Six of them, and each was argued for. `default-features = false` unless a
feature is used — `insta` is here to compare two strings, not to bring serde,
yaml, ron, toml and a glob walker.

`cargo deny` gates advisories, licences and sources; every tolerated advisory
is named with the reason, and the list is meant to shrink. `cargo machete`
catches the dependency nobody removed. Both run in `make audit`.

A new dependency needs to be worth its build time, its advisory surface and
its licence check. `divan` over `criterion` is the example: a tenth of the
dependencies for the four numbers actually wanted.

---

## Standing gaps

Measured 2026-08-15 against rustc 1.96.0, by running clippy with the lints
below turned on. None of these are broken code; they are places where the
crate's stated discipline is not yet mechanically checked. Listed so the
numbers are honest rather than aspirational — re-measure before believing
them, the commands are in the header of each row.

| Check | Hits | Position |
| --- | --- | --- |
| `unreachable_pub` | **0** | Done. Enabled in `Cargo.toml`; twenty-four items became `pub(crate)`. |
| `missing_docs`, `shared` and `tui` | **0** | Done. Enabled per-module. Ninety items in `tui`, sixty-seven in `shared`. |
| doctests | **8** | Done, in the pure layer. The `cargo test --doc` step is no longer running nothing. |
| `clippy::must_use_candidate` | 264 | Partly done: the pure layer has nine. The rest is a judgement call per item, so the lint stays off — enabling it would mark every getter in the crate. |
| `missing_docs`, `github` and `diffline` | 637 | Deliberately out of scope. These are read by whoever is changing them, not by callers. Revisit only if either grows a second consumer. |
| `clippy::indexing_slicing` | 260 | Not enabling. The panic policy above is the answer instead, and it is the harder half — it needs reading, not a lint. |
| `clippy::redundant_clone` | 13 | Not enabling — it is a nursery lint and twelve of the thirteen are it misreading `Display::to_string` on a borrowed error. Checked one by one. The two real ones, in `gh.rs` and `bin/diffline.rs`, are fixed; the survivor at `load.rs:321` is a genuine clone kept on purpose, because it is the last of four identical `insert` lines and breaking the symmetry to save one allocation reads worse than the allocation costs. |

---

## Where these come from

- [Rust API Guidelines] — naming (C-CASE, C-CONV, C-GETTER, C-WORD-ORDER),
  common traits (C-COMMON-TRAITS), error types (C-GOOD-ERR), `Debug`
  everywhere (C-DEBUG), newtypes (C-NEWTYPE, C-CUSTOM-TYPE).
- [RFC 1574] — the shape of a doc comment.
- [jj's style guide] — panics only where an invariant makes them safe; prefer
  lower-level tests to end-to-end ones, which are roughly 100× slower.
- [ripgrep] — a pinned `rustfmt.toml` rather than the version's defaults.
- [uv] — the shape of a lint block: pedantic on, exceptions named one by one
  with a reason. This crate reached the opposite conclusion for its own
  reasons; the format is the part worth copying.
- [helix] — an MSRV that is a policy with a written procedure, not a number
  that drifts.

[Rust API Guidelines]: https://rust-lang.github.io/api-guidelines/checklist.html
[RFC 1574]: https://github.com/rust-lang/rfcs/blob/master/text/1574-more-api-documentation-conventions.md
[jj's style guide]: https://github.com/jj-vcs/jj/blob/main/docs/style_guide.md
[ripgrep]: https://github.com/BurntSushi/ripgrep/blob/master/rustfmt.toml
[uv]: https://github.com/astral-sh/uv/blob/main/Cargo.toml
[helix]: https://github.com/helix-editor/helix/blob/master/docs/CONTRIBUTING.md

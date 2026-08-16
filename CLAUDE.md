# CLAUDE.md

This project keeps its instructions in one place so the two files cannot
disagree.

- **[AGENTS.md](AGENTS.md)** — the map: layout, commands, and the handful of
  rules that get broken by moving fast. Read this first.
- **[CODE-STYLE.md](CODE-STYLE.md)** — the rulebook: panics, errors, naming,
  types, visibility, documentation, tests, dependencies, and where the
  standing gaps are.
- **`src/lib.rs`** — the architecture, as module documentation, with the
  diagram.
- **[README.md](README.md)** — what the two programs do, for whoever runs
  them rather than changes them.

`make check` is the gate before any change is reported as done.

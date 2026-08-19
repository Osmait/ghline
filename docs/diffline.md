# diffline

Reviewing the diff in front of you, and handing the review to a coding agent
as one message.

```sh
diffline            # the repository you are standing in
diffline ~/project  # or one you point it at
```

Three panes: what changed, the diff itself, and a queue of comments anchored to
lines. `[` and `]` step the scope through the working tree, this branch, and
the last commit.

## Comments are anchored to lines, not to rows

A comment belongs to a *line of a file* rather than to a row on screen, because
expanding the context or changing scope renumbers every row. So a note survives
`+`, `r`, and stepping away and back.

A deleted line has no new-side number and is anchored to the old side; a
context line anchors to the new one, since a note on unchanged code is about
the code as it will stand.

## The queue travels as one message

Grouped by file and in line order — the order the agent will work in. Twelve
separate prompts would get twelve separate answers and no shape.

| Key | Action |
| --- | --- |
| `V` | take a range |
| `c` | comment on it |
| `a` | pick an agent |
| `S` | send the queue |
| `?` | every other binding, generated from the live keymap |

Where the message can go, and what happens if the agent will not take it, is
[agents.md](agents.md).

## Watching the working tree

diffline follows the working tree without being restarted: a file saved in your
editor shows up in the review. Native filesystem events rather than polling, so
an idle repository costs nothing.

## Keys

diffline's keymap is a table, not a `match`: `<config>/keys` is read at
startup and applied over the shipped one.

```
<C-n> = line-down
s     = split
j     = none          # takes a key away
```

`:write a keymap to start from` writes every default binding with its action
name and what it does. Keys read as a letter, `<C-d>`, `<leader>x`, `gg`,
`]c`, `<esc>`, `<cr>`, `<tab>`, `<s-tab>`, `<space>` and the arrows; the
leader is space.

A line that names an action that does not exist, or a key that cannot be
read, is skipped and reported at the top of the help — a key that silently
does nothing is worse than one that says why. `␣?` is generated from the map
rather than written down, so it is right the moment you rebind something.

`g`, `z`, `[`, `]` and the leader are prefixes only while something is bound
behind them, so clearing those bindings gives you the key itself back.

# Configuration

Everything is optional and everything is a plain text file you can edit by
hand. Keys a version does not recognise are left alone rather than dropped, so
a config written by a newer build survives an older one.

## Settings

Nine keys in `~/.config/ghline/config`, all optional:

```
prompt      = Work on {repo}#{num}: {title}\n\n{url}\n\n---\n\n{context}
prompt-pr   = Review {repo}#{num}: {title}\n\n{url}\n\n{context}
prompt-run  = Diagnose this failing run in {repo}.\n\n{title}\n{url}\n\n{context}
prompt-diff = Explain this change from {repo}#{num}\n\n{url}\n\n{context}
prompt-file = Here is a file from {repo}.\n\n{url}\n\n---\n\n{context}
agents      = claude, codex, opencode, pi
agent-icons = claude=✳, codex=◆, opencode=◇, pi=π
file-icons  = nerd
clone-roots = ~/code, ~/work
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

## Themes

`t` opens the picker. It applies as you move through it, so what you are judging
is the interface itself rather than a name in a list — `enter` keeps the one you
land on, `esc` puts back the one that was on when you opened it.

The one you keep is remembered: `enter` writes it to
`~/.config/ghline/config` (or `$XDG_CONFIG_HOME`), and the next start reads
it back. The file is `key = value` lines, safe to edit by hand; keys it does not
recognise are left alone rather than dropped, so a config written by a newer
version survives an older one. A theme that cannot be written is still applied,
and says so — silently forgetting looks like a bug. The headless render modes
deliberately ignore it, so a snapshot is the same frame on any machine.

Two ship: the default palette and Catppuccin Mocha. A theme is a whole
`Palette` and switching one in is a single store, so the change lands on the
very next frame. A test walks every theme and fails if a role is left
undefined, which would otherwise show up as an invisible pane.

### Writing your own

Anything in `~/.config/ghline/themes/*.theme` joins the picker, named after
the file. `:write a theme to start from` writes the palette you are looking at
into `themes/mine.theme` with every role listed and commented, which beats
guessing at role names:

```
bg     = #1e1e2e   # the terminal's own background
green  = #a6e3a1   # added, passing, approved
red    = #f38ba8   # deleted, failing, refused
```

`#rrggbb`, `rrggbb` and `#rgb` all read. A `#` starts a comment at the start of
a line or after a value — not inside one, since every value begins with one.
Roles you leave out keep what Mocha gives them, so a theme can be three colours
and a working interface rather than three colours and twenty-six holes; a line
that is not a colour, or names a role that does not exist, is skipped rather
than taking the theme down with it.

The files are read once at startup, so a colour you change shows up on the next
run.

Mocha is mapped by the role each colour plays rather than by name: the default palette
keeps its panels a shade *lighter* than the background, so mantle is the ground
and base is the panel, not the other way round.


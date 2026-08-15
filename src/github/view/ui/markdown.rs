//! Rendering the Markdown that GitHub bodies are written in.
//!
//! `tui-markdown` does the parsing and gives back a ratatui `Text`. What is
//! left is making it look like the rest of the interface: its styles are
//! generic (ANSI blue links, white-on-black inline code) and its lines are
//! unwrapped, so both are mapped onto the design's palette and folded to the
//! pane's width here.

use ratatui::style::{Modifier, Style};
use unicode_width::UnicodeWidthStr;

// `tui-markdown` is built against ratatui-core, whose `Style` is a different
// type from the one this crate draws with even though the values match. The
// bridge below is the whole of that seam.
use ratatui_core::style::{Color as CoreColor, Modifier as CoreModifier, Style as CoreStyle};

use crate::tui::Seg;
use crate::tui::theme;

/// Renders a Markdown body into styled lines of at most `width` columns.
pub fn render(body: &str, width: usize) -> Vec<Vec<Seg>> {
    let prepared = hard_breaks(body);
    let parsed = tui_markdown::from_str(&prepared);
    let mut out = Vec::new();
    let mut fenced = false;

    for line in &parsed.lines {
        let mut spans: Vec<Seg> = line
            .spans
            .iter()
            .map(|s| (s.content.to_string(), restyle(s.style)))
            .collect();

        let plain: String = spans.iter().map(|(t, _)| t.as_str()).collect();
        let trimmed = plain.trim_start();

        // fences are kept as their own markers so the block inside is obvious,
        // but they are not worth a line of their own
        if trimmed.starts_with("```") {
            fenced = !fenced;
            out.push(vec![(
                trimmed.to_string(),
                Style::default().bg(theme::bg()).fg(theme::dimmer()),
            )]);
            continue;
        }

        if !fenced {
            if let Some(h) = heading(&spans) {
                out.push(h);
                continue;
            }
            if let Some(q) = quote(&spans) {
                spans = q;
            }
        }

        // a table or a fenced block loses its meaning if it is folded
        if fenced || preformatted(&plain) {
            out.push(spans);
            continue;
        }
        out.extend(fold(spans, width));
    }
    out
}

/// CommonMark folds a single newline into a space; GitHub does not, and its
/// bodies are written expecting that — an "Expected:" and an "Actual:" line
/// are meant to stay apart. Two trailing spaces are the standard way to ask
/// for the break, so they are added to the lines that need one.
fn hard_breaks(body: &str) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let mut out = String::with_capacity(body.len() + lines.len() * 2);
    let mut fenced = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
        }
        out.push_str(line);

        let next_continues = lines
            .get(i + 1)
            .is_some_and(|n| !n.trim().is_empty() && !n.trim_start().starts_with("```"));
        // a break is only needed inside a run of prose; blocks already break,
        // and a table row would stop being one
        if !fenced
            && next_continues
            && !trimmed.is_empty()
            && !trimmed.starts_with('|')
            && !trimmed.starts_with('#')
            && !line.ends_with("  ")
        {
            out.push_str("  ");
        }
        out.push('\n');
    }
    out
}

/// Maps `tui-markdown`'s generic styling onto the design's palette.
fn restyle(s: CoreStyle) -> Style {
    let base = Style::default().bg(theme::bg());
    let styled = match (s.fg, s.bg) {
        // inline code arrives as white on black
        (Some(CoreColor::White), Some(CoreColor::Black)) => {
            base.bg(theme::tab_active_bg()).fg(theme::cyan_soft())
        }
        // links arrive as ANSI blue; the design paints them cyan
        (Some(CoreColor::Blue), _) => base.fg(theme::cyan()),
        _ => base.fg(theme::body()),
    };
    let mut m = Modifier::empty();
    if s.add_modifier.contains(CoreModifier::BOLD) {
        m |= Modifier::BOLD;
    }
    if s.add_modifier.contains(CoreModifier::ITALIC) {
        m |= Modifier::ITALIC;
    }
    styled.add_modifier(m)
}

/// `# Title` becomes the title, in the colour a heading deserves. GitHub does
/// not show the hashes either.
fn heading(spans: &[Seg]) -> Option<Vec<Seg>> {
    let (marker, _) = spans.first()?;
    let hashes = marker.trim_end();
    if hashes.is_empty() || !hashes.chars().all(|c| c == '#') {
        return None;
    }
    let level = hashes.len();
    let color = if level <= 2 {
        theme::bright()
    } else {
        theme::fg()
    };
    let mut out: Vec<Seg> = spans[1..]
        .iter()
        .map(|(t, _)| {
            (
                t.clone(),
                Style::default()
                    .bg(theme::bg())
                    .fg(color)
                    .add_modifier(Modifier::BOLD),
            )
        })
        .collect();
    if out.is_empty() {
        out.push((String::new(), Style::default().bg(theme::bg())));
    }
    Some(out)
}

/// `> quoted` gets the same left gutter the design uses for comments.
fn quote(spans: &[Seg]) -> Option<Vec<Seg>> {
    let (marker, _) = spans.first()?;
    if marker.trim_end() != ">" {
        return None;
    }
    let mut out = vec![(
        "▌ ".to_string(),
        Style::default().bg(theme::bg()).fg(theme::border()),
    )];
    out.extend(
        spans[1..]
            .iter()
            .map(|(t, s)| (t.clone(), s.fg(theme::dim()))),
    );
    Some(out)
}

/// Table borders and other drawn structures must not be folded.
fn preformatted(plain: &str) -> bool {
    plain.contains(['│', '┌', '└', '├', '┬', '┼', '┴', '┐', '┘', '┤'])
}

/// Folds one styled line to `width` columns, breaking between words and
/// carrying each word's style with it.
fn fold(spans: Vec<Seg>, width: usize) -> Vec<Vec<Seg>> {
    if width == 0 {
        return vec![spans];
    }
    let plain: String = spans.iter().map(|(t, _)| t.as_str()).collect();
    if plain.width() <= width {
        return vec![spans];
    }

    // a list or a quote keeps its marker's indentation on the folded lines
    let indent: String = plain
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect::<String>()
        + &" ".repeat(list_marker(plain.trim_start()));

    let mut out: Vec<Vec<Seg>> = Vec::new();
    let mut line: Vec<Seg> = Vec::new();
    let mut used = 0usize;

    for (text, style) in spans {
        for word in split_keeping_spaces(&text) {
            let w = word.width();
            let blank = word.trim().is_empty();
            if used + w > width && !line.is_empty() && !blank {
                out.push(std::mem::take(&mut line));
                used = indent.width();
                if !indent.is_empty() {
                    line.push((indent.clone(), style));
                }
            }
            if blank && line.is_empty() {
                continue; // no leading space on a folded line
            }
            line.push((word, style));
            used += w;
        }
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

/// Width of a leading `- ` or `1. ` marker, so folded lines line up under the
/// text rather than under the bullet.
fn list_marker(s: &str) -> usize {
    if let Some(rest) = s.strip_prefix("- ").or_else(|| s.strip_prefix("* ")) {
        return s.len() - rest.len();
    }
    let digits = s.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && s[digits..].starts_with(". ") {
        return digits + 2;
    }
    0
}

/// Splits into words while keeping the spaces as their own pieces, so the
/// original spacing survives a fold that does not happen.
fn split_keeping_spaces(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_space = None;
    for c in s.chars() {
        let space = c.is_whitespace();
        if in_space != Some(space) && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        in_space = Some(space);
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(lines: &[Vec<Seg>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.iter().map(|(t, _)| t.as_str()).collect())
            .collect()
    }

    #[test]
    fn a_heading_loses_its_hashes_and_gains_weight() {
        let out = render("## Running containers", 80);
        assert_eq!(plain(&out), vec!["Running containers"]);
        assert!(out[0][0].1.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn emphasis_survives_as_a_modifier() {
        let out = render("text with **bold** in it", 80);
        let bold = out[0]
            .iter()
            .find(|(t, _)| t == "bold")
            .expect("the bold span");
        assert!(bold.1.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn inline_code_and_links_take_the_design_palette() {
        let out = render("a `snippet` and a [link](https://e.com)", 80);
        let spans: Vec<_> = out.iter().flatten().collect();
        assert!(
            spans
                .iter()
                .any(|(t, s)| t == "snippet" && s.fg == Some(theme::cyan_soft())),
            "inline code should not stay white on black"
        );
        assert!(
            spans.iter().any(|(_, s)| s.fg == Some(theme::cyan())),
            "links should be cyan, not ANSI blue"
        );
    }

    #[test]
    fn a_table_is_never_folded() {
        let md = "| a | b |\n|---|---|\n| 1 | 2 |";
        let out = render(md, 4); // far narrower than the table
        for line in plain(&out) {
            if line.contains('│') {
                assert!(line.width() > 4, "the table kept its shape");
            }
        }
    }

    #[test]
    fn a_fenced_block_keeps_its_line_breaks() {
        let md = "```\none\ntwo\n```";
        let out = plain(&render(md, 80));
        assert!(out.contains(&"one".to_string()));
        assert!(out.contains(&"two".to_string()));
    }

    #[test]
    fn long_prose_folds_at_the_width() {
        let out = render("alpha beta gamma delta epsilon", 12);
        let lines = plain(&out);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| l.width() <= 12), "{lines:?}");
    }

    #[test]
    fn a_folded_list_item_lines_up_under_its_text() {
        let out = plain(&render("- alpha beta gamma delta epsilon zeta", 16));
        assert!(out.len() > 1);
        assert!(
            out[1].starts_with("  "),
            "continuation is indented: {out:?}"
        );
    }

    #[test]
    fn a_single_newline_still_breaks_the_line() {
        // GitHub renders these apart; plain CommonMark would join them
        let out = plain(&render("Expected: it collapses.\nActual: it panics.", 80));
        assert!(out.iter().any(|l| l.starts_with("Expected:")), "{out:?}");
        assert!(out.iter().any(|l| l.starts_with("Actual:")), "{out:?}");
    }

    #[test]
    fn hard_breaks_leave_tables_and_fences_alone() {
        let md = "| a | b |\n|---|---|\n| 1 | 2 |";
        assert!(
            !hard_breaks(md).contains("|  \n"),
            "a table row stays a row"
        );

        let fence = "```\none\ntwo\n```";
        assert!(
            !hard_breaks(fence).contains("one  "),
            "code keeps its own spacing"
        );
    }

    #[test]
    fn an_empty_body_renders_nothing_surprising() {
        assert!(render("", 40).len() <= 1);
    }
}

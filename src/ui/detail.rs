//! Detail view: an issue (one column) and a PR / run (two columns + checks).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::{
    Seg, bold, fill, hline, markdown, pct, put, put_right, put_trunc, skel_bar, vline, wrap,
};
use crate::app::{App, Pane};
use crate::data::{Kind, Status};
use crate::theme;

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App) {
    let Some(cur) = app.current() else { return };
    if cur.kind() == Kind::Issue {
        issue(buf, area, app);
    } else {
        pull(buf, area, app);
    }
}

fn state_badge(buf: &mut Buffer, x: u16, y: u16, max: u16, state: Status) -> u16 {
    let color = theme::state_color(state);
    let base = Style::default().bg(theme::bg()).fg(color);
    let text = format!("[ {} ]", state.label().to_uppercase());
    put(buf, x, y, max, &text, base)
}

/// Draws a window into `lines` starting at `offset` and, if there is more
/// content than fits, a scrollbar down the right edge.
fn window(buf: &mut Buffer, area: Rect, lines: &[Vec<Seg>], offset: usize, focused: bool) {
    let h = area.height as usize;
    let more = lines.len() > h;
    // the scrollbar claims the last column when it is needed
    let right = if more {
        area.right().saturating_sub(2)
    } else {
        area.right()
    };

    for (row, line) in lines.iter().skip(offset).take(h).enumerate() {
        let y = area.y + row as u16;
        let mut cx = area.x;
        for (text, style) in line {
            cx = put_trunc(buf, cx, y, right, text, *style);
        }
    }

    if focused {
        // panes with no selection mark focus with the same bar the selected
        // rows use everywhere else in the interface
        for i in 0..h {
            put(
                buf,
                area.x.saturating_sub(2),
                area.y + i as u16,
                area.x,
                "▌",
                Style::default().bg(theme::bg()).fg(theme::cyan()),
            );
        }
    }

    if !more {
        return;
    }
    let bx = area.right() - 1;
    let track = Style::default().bg(theme::bg()).fg(theme::border());
    let thumb =
        Style::default()
            .bg(theme::bg())
            .fg(if focused { theme::cyan() } else { theme::dim() });
    let max = lines.len() - h;
    let size = ((h * h) / lines.len()).max(1);
    let pos = (h - size).checked_mul(offset).map_or(0, |v| v / max.max(1));
    for i in 0..h {
        let inside = i >= pos && i < pos + size;
        put(
            buf,
            bx,
            area.y + i as u16,
            area.right(),
            if inside { "┃" } else { "│" },
            if inside { thumb } else { track },
        );
    }
}

/// Placeholder lines for a body that has not arrived. They go through the same
/// line model as the real content, so the pane scrolls and clips identically.
///
/// `first` offsets the highlight band so separate sections do not pulse in
/// lockstep.
fn skeleton_lines(width: usize, widths: &[u16], first: usize) -> Vec<Vec<Seg>> {
    widths
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let w = (width * p as usize / 100).max(1);
            vec![(
                "█".repeat(w),
                Style::default().bg(theme::bg()).fg(band(i + first)),
            )]
        })
        .collect()
}

/// The skeleton's resting colour. The travelling highlight lives in
/// `ui::skel_bar`; these lines are static because they scroll with the pane and
/// a moving band would fight the scrolling.
fn band(_row: usize) -> ratatui::style::Color {
    theme::sel()
}

/// How far a pane of `height` rows can scroll.
fn max_offset(len: usize, height: u16) -> usize {
    len.saturating_sub(height as usize)
}

fn issue_lines(cur: &crate::data::Item, width: usize, pending: bool) -> Vec<Vec<Seg>> {
    let base = Style::default().bg(theme::bg());
    let mut out: Vec<Vec<Seg>> = Vec::new();

    out.push(vec![
        (
            format!("[ {} ]", cur.state.label().to_uppercase()),
            base.fg(theme::state_color(cur.state)),
        ),
        (format!("  #{}  ", cur.num), base.fg(theme::dimmer())),
        (
            format!("{} · {}", cur.author, cur.when),
            base.fg(theme::dimmer()),
        ),
    ]);
    out.push(Vec::new());

    for l in wrap(&cur.title, width.min(70)) {
        out.push(vec![(l, bold(base.fg(theme::bright())))]);
    }
    out.push(Vec::new());
    out.push(vec![("┄".repeat(width), base.fg(theme::border()))]);
    out.push(Vec::new());

    if pending && cur.body.is_empty() {
        out.extend(skeleton_lines(width.min(76), &[82, 70, 88, 54, 76, 40], 0));
    } else {
        out.extend(markdown::render(&cur.body_text(), width.min(76)));
    }
    out.push(Vec::new());
    out.push(vec![("COMMENTS".into(), base.fg(theme::dim()))]);
    out.push(Vec::new());
    if pending && cur.as_issue().is_none_or(|i| i.comment_list.is_empty()) {
        out.extend(skeleton_lines(width.min(76), &[30, 68, 26, 74], 7));
    }

    for c in cur.as_issue().into_iter().flat_map(|i| &i.comment_list) {
        out.push(vec![
            ("▌ ".into(), base.fg(theme::border())),
            (c.author.clone(), base.fg(theme::yellow())),
            (format!(" · {}", c.when), base.fg(theme::dimmer())),
        ]);
        for l in wrap(&c.body, width.saturating_sub(2).min(76)) {
            out.push(vec![
                ("▌ ".into(), base.fg(theme::border())),
                (l, base.fg(theme::body())),
            ]);
        }
        out.push(Vec::new());
    }
    out
}

fn issue(buf: &mut Buffer, area: Rect, app: &mut App) {
    let Some(cur) = app.current() else { return };
    let inner = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(3),
        height: area.height.saturating_sub(1),
    };
    let lines = issue_lines(
        cur,
        inner.width.saturating_sub(2) as usize,
        app.detail_status().is_loading(),
    );
    app.detail_height = inner.height;
    app.detail_scroll = app.detail_scroll.min(max_offset(lines.len(), inner.height));
    window(
        buf,
        inner,
        &lines,
        app.detail_scroll,
        app.pane == Pane::Body,
    );
}

fn pull(buf: &mut Buffer, area: Rect, app: &mut App) {
    let Some(cur) = app.current() else { return };
    let base = Style::default().bg(theme::bg());
    let x = area.x + 2;
    let max = area.right() - 2;

    // ---- header
    let mut y = area.y + 1;
    let mut cx = state_badge(buf, x, y, max, cur.state);
    cx = put(buf, cx, y, max, "  ", base);
    cx = put(
        buf,
        cx,
        y,
        max,
        &format!("#{}", cur.num),
        base.fg(theme::dimmer()),
    );
    cx = put(buf, cx, y, max, "  ", base);
    cx = put(
        buf,
        cx,
        y,
        max,
        &format!("{} · {}", cur.author, cur.when),
        base.fg(theme::dimmer()),
    );
    cx = put(buf, cx, y, max, "  ", base);
    let (checks_label, checks_color) = if cur.kind() == Kind::Pr {
        (
            format!(
                "{} {} checks",
                theme::state_icon(cur.checks()),
                cur.checks()
            ),
            theme::state_color(cur.checks()),
        )
    } else {
        (
            format!("{} {}", theme::state_icon(cur.state), cur.state),
            theme::state_color(cur.state),
        )
    };
    put(buf, cx, y, max, &checks_label, base.fg(checks_color));

    y += 1;
    for line in wrap(&cur.title, (max - x).min(70) as usize) {
        put(buf, x, y, max, &line, bold(base.fg(theme::bright())));
        y += 1;
    }

    let pr = cur.as_pr();
    let branch_line = if !cur.branch().is_empty() {
        let verb = match cur.state {
            Status::Merged => "merged",
            Status::Closed => "closed",
            _ => "merge",
        };
        format!(
            "{verb} {} → main · {} {} across {} files",
            cur.branch(),
            pr.map_or("", |p| p.add.as_str()),
            pr.map_or("", |p| p.del.as_str()),
            pr.map_or(0, |p| p.files)
        )
    } else {
        let run = cur.as_run();
        format!(
            "{} · {}",
            run.map_or("", |r| r.event.as_str()),
            run.map_or("", |r| r.dur.as_str())
        )
    };
    let bx = put_trunc(buf, x, y, max, &branch_line, base.fg(theme::dimmer()));

    // outcome of the actions: merge method and branch state
    if let Some(method) = pr.and_then(|p| p.merged_with.as_ref()) {
        let cx = put(buf, bx, y, max, "  ·  ", base.fg(theme::dimmer()));
        put(buf, cx, y, max, method, base.fg(theme::purple()));
    }
    if pr.is_some_and(|p| p.branch_deleted) {
        let cx = put(
            buf,
            max.saturating_sub(16),
            y,
            max,
            "⊘ ",
            base.fg(theme::red()),
        );
        put(buf, cx, y, max, "branch deleted", base.fg(theme::dimmer()));
    }
    y += 1;

    hline(buf, area.x, y, area.width, theme::border());
    y += 1;

    // ---- two columns (1fr | 1.15fr)
    let body_h = area.bottom().saturating_sub(y);
    if body_h == 0 {
        return;
    }
    let left_w = ((area.width as f32 - 1.0) / 2.15).round() as u16;
    let left = Rect {
        x: area.x,
        y,
        width: left_w,
        height: body_h,
    };
    let right = Rect {
        x: area.x + left_w + 1,
        y,
        width: area.width - left_w - 1,
        height: body_h,
    };
    vline(buf, area.x + left_w, y, body_h, theme::border());

    description(buf, left, app);
    checks_pane(buf, right, app);
}

fn description_lines(cur: &crate::data::Item, width: usize, pending: bool) -> Vec<Vec<Seg>> {
    let base = Style::default().bg(theme::bg());
    let mut out: Vec<Vec<Seg>> = Vec::new();

    out.push(vec![("DESCRIPTION".into(), base.fg(theme::dim()))]);
    // a skeleton only while there is nothing to show; a refresh keeps the old
    // body on screen rather than blanking it
    if pending && cur.body.is_empty() {
        // saying "no description" here would be a lie, so the shape of one is
        // drawn instead
        out.extend(skeleton_lines(width, &[76, 88, 64, 80, 42], 0));
    } else {
        out.extend(markdown::render(&cur.body_text(), width));
    }

    out.push(Vec::new());
    out.push(vec![
        ("FILES CHANGED".into(), base.fg(theme::dim())),
        ("   d → diff".into(), base.fg(theme::dimmer())),
    ]);
    if pending && cur.files().is_empty() {
        out.extend(skeleton_lines(width, &[58, 44, 66, 50], 5));
    }
    for f in cur.files() {
        // the counts hug the right edge of the available width
        let stats = format!("{} {}", f.add, f.del);
        let room = width.saturating_sub(stats.chars().count() + 1);
        let path = super::truncate_pad(&f.path, room);
        out.push(vec![
            (path, base.fg(theme::body())),
            (" ".into(), base),
            (f.add.clone(), base.fg(theme::green())),
            (" ".into(), base),
            (f.del.clone(), base.fg(theme::red())),
        ]);
    }

    out.push(Vec::new());
    out.push(vec![("REVIEWS".into(), base.fg(theme::dim()))]);
    let reviews_empty = cur.as_pr().is_none_or(|p| p.reviews.is_empty());
    if pending && reviews_empty {
        out.extend(skeleton_lines(width, &[34, 28], 9));
    }
    for r in cur.as_pr().into_iter().flat_map(|p| &p.reviews) {
        let (color, icon) = theme::review(r.state);
        out.push(vec![
            (format!("{icon} "), base.fg(color)),
            (format!("{} ", r.author), base.fg(theme::body())),
            (r.state.label().to_string(), base.fg(theme::dimmer())),
        ]);
    }
    out
}

fn description(buf: &mut Buffer, area: Rect, app: &mut App) {
    let Some(cur) = app.current() else { return };
    let inner = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(3),
        height: area.height.saturating_sub(1),
    };
    let lines = description_lines(
        cur,
        inner.width.saturating_sub(2) as usize,
        app.detail_status().is_loading(),
    );
    app.detail_height = inner.height;
    app.detail_scroll = app.detail_scroll.min(max_offset(lines.len(), inner.height));
    window(
        buf,
        inner,
        &lines,
        app.detail_scroll,
        app.pane == Pane::Body,
    );
}

fn checks_pane(buf: &mut Buffer, area: Rect, app: &App) {
    let Some(cur) = app.current() else { return };
    fill(buf, area, theme::panel_alt());

    // pane header
    let head = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    fill(buf, head, theme::panel());
    let hs = Style::default().bg(theme::panel()).fg(theme::dim());
    let workflow = if app.live() {
        let name = if cur.workflow().is_empty() {
            "checks"
        } else {
            cur.workflow()
        };
        if cur.id > 0 {
            format!("{name} · run {}", cur.id)
        } else {
            name.to_string()
        }
    } else if cur.kind() == Kind::Pr {
        format!("CI #{}", 1841 - app.repo_idx())
    } else {
        format!(
            "{} #{}",
            cur.title.split(" · ").next().unwrap_or(&cur.title),
            cur.num
        )
    };
    put_trunc(
        buf,
        area.x + 1,
        area.y,
        area.right() - 14,
        &format!("CHECKS · {workflow}"),
        hs,
    );
    put_right(
        buf,
        area.right() - 1,
        area.y,
        "enter → logs",
        hs.fg(theme::dimmer()),
    );
    hline(buf, area.x, area.y + 1, area.width, theme::border_soft());

    // jobs
    let jobs = app.jobs();
    let mut y = area.y + 2;
    if jobs.is_empty() {
        let st = app.jobs_status();
        if st.is_loading() {
            let avail = area.width.saturating_sub(18);
            let names = [32, 48, 42, 37, 27];
            for (row, &name) in names.iter().enumerate() {
                let ry = y + row as u16;
                skel_bar(buf, area.x + 2, ry, 1, row, app.anim);
                skel_bar(buf, area.x + 4, ry, pct(avail, name), row, app.anim);
                skel_bar(buf, area.right().saturating_sub(11), ry, 9, row, app.anim);
            }
            return;
        }
        let (msg, color) = match st.error() {
            Some(e) => (e.to_string(), theme::red()),
            None => (
                "no checks for this pull request".to_string(),
                theme::dimmer(),
            ),
        };
        put_trunc(
            buf,
            area.x + 2,
            y,
            area.right() - 1,
            &msg,
            Style::default().bg(theme::panel_alt()).fg(color),
        );
        return;
    }
    for (i, j) in jobs.iter().enumerate() {
        if y >= area.bottom() {
            return;
        }
        let sel = i == app.check;
        let bg = if sel {
            theme::sel()
        } else {
            theme::panel_alt()
        };
        fill(
            buf,
            Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            },
            bg,
        );
        let base = Style::default().bg(bg);
        if sel {
            let mark = if app.pane == Pane::Checks {
                theme::cyan()
            } else {
                theme::sel_mark_idle()
            };
            put(buf, area.x, y, area.right(), "▌", base.fg(mark));
        }
        let color = theme::state_color(j.status);
        put(
            buf,
            area.x + 2,
            y,
            area.right(),
            theme::state_icon(j.status),
            base.fg(color),
        );

        let dur_x = put_right(buf, area.right() - 1, y, &j.dur, base.fg(theme::dimmer()));
        let st_x = put_right(buf, dur_x - 2, y, j.status.label(), base.fg(color));
        let fg = if sel { theme::bright() } else { theme::fg() };
        put_trunc(buf, area.x + 4, y, st_x - 1, &j.name, base.fg(fg));
        y += 1;
    }

    // summary
    y += 1;
    if y + 2 >= area.bottom() {
        return;
    }
    let base = Style::default().bg(theme::panel_alt());
    let w = area.width.saturating_sub(4);
    let s = "┄".repeat(w as usize);
    put(
        buf,
        area.x + 2,
        y,
        area.right(),
        &s,
        base.fg(theme::border()),
    );
    y += 1;

    let passed = jobs.iter().filter(|j| j.status == Status::Success).count();
    let failed = jobs.iter().filter(|j| j.status == Status::Failure).count();
    let progress = jobs
        .iter()
        .filter(|j| j.status == Status::Running || j.status == Status::Pending)
        .count();
    put_trunc(
        buf,
        area.x + 2,
        y,
        area.right() - 1,
        &format!("{passed} passed · {failed} failed · {progress} in progress"),
        base.fg(theme::dimmer()),
    );
    y += 1;
    let trigger = if app.live() {
        let event = if cur.as_run().map_or("", |r| r.event.as_str()).is_empty() {
            "pull_request"
        } else {
            cur.as_run().map_or("", |r| r.event.as_str())
        };
        format!("{event} · {} · {}", cur.author, cur.when)
    } else {
        format!(
            "runner: ubuntu-24.04 / macos-15 · billable 6m 20s · queued {}",
            cur.when
        )
    };
    put_trunc(
        buf,
        area.x + 2,
        y,
        area.right() - 1,
        &trigger,
        base.fg(theme::dimmest()),
    );
}

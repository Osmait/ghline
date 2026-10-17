//! Every command diffline advertises, held to what it says it does.
//!
//! The help is generated from `keys::DEFAULTS` and `Action::about`, the
//! palette from `input::COMMANDS`, and the rest of what the screen offers is
//! written into the status bar, the panes and the modals. Each of those is a
//! promise made to somebody reading the screen, and this module keeps one
//! test per promise, pressed the way that reader would press it. A test
//! marked `#[ignore = "BROKEN: …"]` is a promise the program does not keep
//! yet; it is kept here so the fix has a test waiting for it.

use std::cell::RefCell;
use std::rc::Rc;

use crate::app::{App, FinderTab, Load, Modal, Pane, Pending};
use crate::keys::{self, Action};
use crate::model::{ChangedFile, Comment, Kind, Row, Scope, State, Status};
use crate::service::{Request, Response, Write};
use crate::shared::key::{Key, Press};
use crate::shared::nav::Dir;
use crate::shared::worker::{Gone, Worker};

// --- fixtures ---

/// The requests a test's app sent, in order.
type Sent = Rc<RefCell<Vec<Request>>>;

/// A worker that writes down what it was asked and never answers: what these
/// tests check is that the right request left, not what came back.
struct Recording(Sent);

impl Worker<Request, Response> for Recording {
    fn send(&self, req: Request) -> bool {
        self.0.borrow_mut().push(req);
        true
    }

    fn poll(&self) -> Result<Option<Response>, Gone> {
        Ok(None)
    }
}

fn recording(a: &mut App) -> Sent {
    let sent = Sent::default();
    a.service = Some(Box::new(Recording(Rc::clone(&sent))));
    sent
}

fn press(app: &mut App, key: Key) {
    app.on_key(Press::new(key));
}

fn typed(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, Key::Char(c));
    }
}

fn bracket(a: &mut App, open: char, c: char) {
    press(a, Key::Char(open));
    press(a, Key::Char(c));
}

/// Presses `spec` as a config file spells it — `j`, `<C-d>`, `<leader>n`,
/// `gg`, `]c` — prefix first, the way a hand does.
fn chord(a: &mut App, spec: &str) {
    let c = keys::parse_chord(spec).unwrap_or_else(|| panic!("{spec} is not a chord"));
    let lead = match c.prefix {
        Pending::None => None,
        Pending::Leader => Some(' '),
        Pending::G => Some('g'),
        Pending::Z => Some('z'),
        Pending::Bracket(Dir::Prev) => Some('['),
        Pending::Bracket(Dir::Next) => Some(']'),
    };
    if let Some(k) = lead {
        press(a, Key::Char(k));
    }
    a.on_key(c.press);
}

/// Presses the shipped key for `action`, after checking it is the shipped
/// key: a test that pressed a chord the keymap no longer has would pass for
/// the wrong reason.
fn act(a: &mut App, spec: &str, action: Action) {
    assert!(
        keys::DEFAULTS.contains(&(spec, action)),
        "{spec} is not {} in the shipped keymap",
        action.name()
    );
    chord(a, spec);
}

/// Runs a palette entry the way a reader does: `:`, its name, enter.
fn palette(a: &mut App, label: &str) {
    act(a, ":", Action::Commands);
    assert_eq!(a.modal, Some(Modal::Palette));
    typed(a, label);
    assert_eq!(
        a.palette_hits().first().map(String::as_str),
        Some(label),
        "typing the whole name should put that entry first"
    );
    press(a, Key::Enter);
}

/// A diff already in place and no worker, as `input`'s tests have it — with
/// the settings in memory and the shipped keymap, so neither this machine's
/// config nor its keys file can change what a key does here.
fn app() -> App {
    let _ = crate::shared::settings::use_store(Box::<crate::shared::settings::Memory>::default());
    let mut a = App::new(
        "/tmp/r".into(),
        Scope::WorkingTree,
        vec![Scope::WorkingTree],
        None,
    );
    a.keys = keys::Map::new();
    a.files = vec![
        ChangedFile {
            path: "src/a.rs".into(),
            status: Status::Modified,
            add: 2,
            del: 1,
        },
        ChangedFile {
            path: "src/b.rs".into(),
            status: Status::Added,
            add: 9,
            del: 0,
        },
    ];
    a.files_state = Load::Ready;
    let rows = vec![
        row(Kind::Header, None, None, "@@ -1,3 +1,4 @@"),
        row(Kind::Context, Some(1), Some(1), "fn main() {"),
        row(Kind::Deleted, Some(2), None, "  old();"),
        row(Kind::Added, None, Some(2), "  new();"),
        row(Kind::Added, None, Some(3), "  more();"),
    ];
    a.rows.insert("src/a.rs".into(), rows);
    a.rows_state.insert("src/a.rs".into(), Load::Ready);
    a.cursor = 1;
    a
}

fn row(kind: Kind, old: Option<u32>, new: Option<u32>, text: &str) -> Row {
    Row {
        kind,
        old,
        new,
        text: text.into(),
    }
}

/// `src/a.rs` as one hunk of forty lines, on a window ten high: enough room
/// for the motions that are about the screen rather than the file. Row `i`
/// is line `i`; row 0 is the header.
fn long() -> App {
    let mut a = app();
    let mut rows = vec![row(Kind::Header, None, None, "@@ -1,40 +1,40 @@")];
    for i in 1..=40 {
        rows.push(row(Kind::Context, Some(i), Some(i), &format!("line {i}")));
    }
    a.rows.insert("src/a.rs".into(), rows);
    a.view_height = 10;
    a.cursor = 1;
    a
}

/// The fixture's `src/a.rs` with a second hunk after its first.
fn two_hunks() -> App {
    let mut a = app();
    let mut rows = a.rows["src/a.rs"].clone();
    rows.push(row(Kind::Header, None, None, "@@ -20,2 +21,2 @@"));
    rows.push(row(Kind::Context, Some(20), Some(21), "fn later() {"));
    a.rows.insert("src/a.rs".into(), rows);
    a.cursor = 1;
    a
}

/// Three places to stand: the tree, the branch, a commit.
fn three_scopes() -> App {
    let mut a = app();
    a.scopes = vec![
        Scope::WorkingTree,
        Scope::Branch {
            base: "main".into(),
        },
        Scope::Commit {
            sha: "c".repeat(40),
        },
    ];
    a
}

/// A note queued on the cursor's line.
fn queue_note(a: &mut App, body: &str) {
    let anchors = a.selected_anchors();
    assert!(!anchors.is_empty(), "the fixture needs a code line");
    a.comments.push(Comment {
        anchors,
        file: a.path().to_string(),
        snippet: String::new(),
        body: body.into(),
        state: State::Queued,
    });
}

fn idle_agent(kind: &str) -> crate::shared::mux::Agent {
    crate::shared::mux::Agent {
        kind: kind.into(),
        pane: format!("w:{kind}"),
        cwd: "/tmp/r".into(),
        status: crate::shared::mux::AgentStatus::Idle,
        title: String::new(),
        focused: false,
    }
}

fn commit(fill: char, subject: &str) -> crate::model::LogEntry {
    crate::model::LogEntry {
        sha: fill.to_string().repeat(40),
        subject: subject.into(),
        author: "Maria".into(),
        when: 1_700_000_000,
    }
}

/// Asks the app for whatever it now needs, as the loop does each frame, and
/// hands back what it asked for.
fn wants(a: &mut App, sent: &Sent) -> Vec<Request> {
    a.ensure();
    sent.borrow_mut().drain(..).collect()
}

/// Everything a key could change that a reader could see, as one value, so
/// two ways of doing the same thing can be compared whole.
fn seen(a: &App) -> String {
    format!(
        "cursor {} scroll {} h {} pane {:?} modal {:?} anchor {:?} split {} blame {} \
         ctx {} ws {} tree {} queue {} quit {} redraw {} toast {:?} scope {} file {} \
         rows {} sel {} help {} tab {:?} folds {:?} comments {} pending {:?}",
        a.cursor,
        a.diff_scroll,
        a.hscroll,
        a.pane,
        a.modal,
        a.anchor,
        a.split,
        a.blame_on,
        a.context,
        a.ignore_ws,
        a.tree_shown,
        a.queue_shown,
        a.should_quit,
        a.wants_redraw,
        a.toast,
        a.scope,
        a.file_idx,
        a.tree_rows().len(),
        a.sel,
        a.help_scroll,
        a.finder_tab,
        a.tree_folds,
        a.comments.len(),
        a.pending,
    )
}

fn draw(a: &mut App) -> String {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let mut term = Terminal::new(TestBackend::new(140, 24)).unwrap();
    term.draw(|f| crate::view::draw(f, a)).unwrap();
    crate::tui::probe::screen(&term)
}

/// Wide enough for the queue, which only shows from 150 columns.
fn draw_wide(a: &mut App) -> String {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let mut term = Terminal::new(TestBackend::new(170, 24)).unwrap();
    term.draw(|f| crate::view::draw(f, a)).unwrap();
    crate::tui::probe::screen(&term)
}

// --- the keymap table itself ---

#[test]
fn every_shipped_chord_is_a_chord_the_keymap_reads() {
    for (spec, action) in keys::DEFAULTS {
        let c = keys::parse_chord(spec);
        assert!(c.is_some(), "{spec} does not parse");
        assert_eq!(
            keys::Map::new().get(c.unwrap()),
            Some(*action),
            "{spec} is not bound to {}",
            action.name()
        );
    }
}

#[test]
fn every_shipped_chord_does_exactly_what_its_action_does() {
    // The dispatch half of the promise: whatever the help says a key does,
    // pressing that key and running that action leave the same state behind.
    for (spec, action) in keys::DEFAULTS {
        let mut by_key = app();
        chord(&mut by_key, spec);
        let mut by_action = app();
        by_action.run(*action, 1);
        assert_eq!(
            seen(&by_key),
            seen(&by_action),
            "{spec} did not do {}",
            action.name()
        );
    }
}

#[test]
fn every_action_says_what_it_does_and_has_a_name_a_file_can_use() {
    for action in keys::ALL {
        assert!(!action.about().is_empty(), "{action:?}");
        assert_eq!(keys::Action::from_name(action.name()), Some(*action));
    }
}

// --- motions ---

#[test]
fn j_is_line_down() {
    let mut a = app();
    act(&mut a, "j", Action::LineDown);
    assert_eq!(a.cursor, 2);
}

#[test]
fn the_down_arrow_is_line_down() {
    let mut a = app();
    act(&mut a, "<down>", Action::LineDown);
    assert_eq!(a.cursor, 2);
}

#[test]
fn gj_is_line_down() {
    let mut a = app();
    act(&mut a, "gj", Action::LineDown);
    assert_eq!(a.cursor, 2);
}

#[test]
fn k_is_line_up() {
    let mut a = app();
    a.cursor = 3;
    act(&mut a, "k", Action::LineUp);
    assert_eq!(a.cursor, 2);
}

#[test]
fn the_up_arrow_is_line_up() {
    let mut a = app();
    a.cursor = 3;
    act(&mut a, "<up>", Action::LineUp);
    assert_eq!(a.cursor, 2);
}

#[test]
fn gk_is_line_up() {
    let mut a = app();
    a.cursor = 3;
    act(&mut a, "gk", Action::LineUp);
    assert_eq!(a.cursor, 2);
}

#[test]
fn gg_is_the_first_line() {
    let mut a = app();
    a.cursor = 4;
    act(&mut a, "gg", Action::Top);
    assert_eq!(a.cursor, 1, "the first line, not the hunk header above it");
}

#[test]
fn capital_g_is_the_last_line() {
    let mut a = app();
    act(&mut a, "G", Action::Bottom);
    assert_eq!(a.cursor, 4);
}

#[test]
fn capital_h_is_the_top_of_the_screen() {
    let mut a = long();
    a.diff_scroll = 10;
    a.cursor = 15;
    act(&mut a, "H", Action::ScreenTop);
    assert_eq!(a.cursor, 10);
}

#[test]
fn capital_m_is_the_middle_of_the_screen() {
    let mut a = long();
    a.diff_scroll = 10;
    a.cursor = 10;
    act(&mut a, "M", Action::ScreenMiddle);
    assert_eq!(a.cursor, 14, "rows 10 to 19 are on screen");
}

#[test]
fn capital_l_is_the_bottom_of_the_screen() {
    let mut a = long();
    a.diff_scroll = 10;
    a.cursor = 10;
    act(&mut a, "L", Action::ScreenBottom);
    assert_eq!(a.cursor, 19);
}

#[test]
fn ctrl_d_is_half_a_screen_down() {
    let mut a = long();
    act(&mut a, "<C-d>", Action::HalfDown);
    assert_eq!(a.cursor, 6, "five lines on a window ten high");
}

#[test]
fn ctrl_u_is_half_a_screen_up() {
    let mut a = long();
    a.cursor = 20;
    act(&mut a, "<C-u>", Action::HalfUp);
    assert_eq!(a.cursor, 15);
}

#[test]
fn ctrl_f_is_a_screen_down() {
    let mut a = long();
    act(&mut a, "<C-f>", Action::PageDown);
    assert_eq!(a.cursor, 11);
}

#[test]
fn page_down_is_a_screen_down() {
    let mut a = long();
    act(&mut a, "<pagedown>", Action::PageDown);
    assert_eq!(a.cursor, 11);
}

#[test]
fn ctrl_b_is_a_screen_up() {
    let mut a = long();
    a.cursor = 20;
    act(&mut a, "<C-b>", Action::PageUp);
    assert_eq!(a.cursor, 10);
}

#[test]
fn page_up_is_a_screen_up() {
    let mut a = long();
    a.cursor = 20;
    act(&mut a, "<pageup>", Action::PageUp);
    assert_eq!(a.cursor, 10);
}

#[test]
fn ctrl_e_scrolls_down_and_the_cursor_stays() {
    let mut a = long();
    a.diff_scroll = 10;
    a.cursor = 15;
    act(&mut a, "<C-e>", Action::ViewDown);
    assert_eq!(a.diff_scroll, 11);
    assert_eq!(a.cursor, 15);
}

#[test]
fn ctrl_y_scrolls_up_and_the_cursor_stays() {
    let mut a = long();
    a.diff_scroll = 10;
    a.cursor = 15;
    act(&mut a, "<C-y>", Action::ViewUp);
    assert_eq!(a.diff_scroll, 9);
    assert_eq!(a.cursor, 15);
}

#[test]
fn a_closing_brace_is_the_next_hunk() {
    let mut a = two_hunks();
    act(&mut a, "}", Action::HunkNext);
    assert_eq!(a.cursor, 6, "the second hunk's first line");
}

#[test]
fn an_opening_brace_is_the_previous_hunk() {
    let mut a = two_hunks();
    a.cursor = 6;
    act(&mut a, "{", Action::HunkPrev);
    assert_eq!(a.cursor, 1);
}

#[test]
fn bracket_c_forward_is_the_next_change() {
    let mut a = app();
    act(&mut a, "]c", Action::ChangeNext);
    assert_eq!(a.cursor, 2, "the first changed line");
}

#[test]
fn bracket_c_back_is_the_previous_change_landing_on_its_start() {
    let mut a = two_hunks();
    a.cursor = 6;
    act(&mut a, "[c", Action::ChangePrev);
    assert_eq!(a.cursor, 2, "the start of the run, not its end");
}

#[test]
fn bracket_f_forward_is_the_next_file() {
    let mut a = app();
    act(&mut a, "]f", Action::FileNext);
    assert_eq!(a.path(), "src/b.rs");
    assert_eq!(a.pane, Pane::Diff);
}

#[test]
fn bracket_f_back_is_the_previous_file() {
    let mut a = app();
    a.goto_file(1);
    act(&mut a, "[f", Action::FilePrev);
    assert_eq!(a.path(), "src/a.rs");
}

#[test]
fn capital_o_folds_every_folder_and_then_opens_every_one() {
    let mut a = app();
    a.pane = Pane::Tree;
    assert_eq!(a.tree_rows().len(), 3, "src, a.rs, b.rs");
    act(&mut a, "O", Action::FoldAll);
    assert_eq!(a.tree_rows().len(), 1, "every folder folded");
    act(&mut a, "O", Action::FoldAll);
    assert_eq!(a.tree_rows().len(), 3, "and every one open again");
}

#[test]
fn leader_w_leaves_whitespace_changes_out_of_the_diff() {
    let mut a = app();
    let sent = recording(&mut a);
    act(&mut a, "<leader>w", Action::IgnoreWhitespace);
    assert!(a.ignore_ws);
    assert_eq!(a.toast, "whitespace changes ignored");
    let asked = wants(&mut a, &sent);
    assert!(
        asked
            .iter()
            .any(|r| matches!(r, Request::Diff { opts, .. } if opts.ignore_ws)),
        "the diff is asked for again without whitespace"
    );
    act(&mut a, "<leader>w", Action::IgnoreWhitespace);
    assert!(!a.ignore_ws, "and the second press puts it back");
}

#[test]
fn bracket_s_forward_is_the_next_scope() {
    let mut a = three_scopes();
    let sent = recording(&mut a);
    act(&mut a, "]s", Action::ScopeNext);
    assert_eq!(
        a.scope,
        Scope::Branch {
            base: "main".into()
        }
    );
    assert!(a.toast.starts_with("scope →"), "{}", a.toast);
    assert!(
        wants(&mut a, &sent)
            .iter()
            .any(|r| matches!(r, Request::Files { scope, .. } if *scope == a.scope)),
        "and its files are asked for"
    );
}

#[test]
fn bracket_s_back_is_the_previous_scope() {
    let mut a = three_scopes();
    act(&mut a, "[s", Action::ScopePrev);
    assert!(
        matches!(a.scope, Scope::Commit { .. }),
        "round the start to the last"
    );
}

#[test]
fn zz_puts_the_cursor_line_in_the_middle() {
    let mut a = long();
    a.cursor = 20;
    act(&mut a, "zz", Action::CursorToMiddle);
    assert_eq!(a.diff_scroll, 15);
    assert_eq!(a.cursor, 20, "the window moved, not the cursor");
}

#[test]
fn zt_puts_the_cursor_line_at_the_top() {
    let mut a = long();
    a.cursor = 20;
    act(&mut a, "zt", Action::CursorToTop);
    assert_eq!(a.diff_scroll, 20);
}

#[test]
fn zb_puts_the_cursor_line_at_the_bottom() {
    let mut a = long();
    a.cursor = 20;
    act(&mut a, "zb", Action::CursorToBottom);
    assert_eq!(a.diff_scroll, 11, "rows 11 to 20 on a window ten high");
}

#[test]
fn h_scrolls_the_line_left() {
    let mut a = app();
    a.hscroll = 5;
    act(&mut a, "h", Action::ScrollLeft);
    assert_eq!(a.hscroll, 4);
    assert_eq!(a.pane, Pane::Diff);
}

#[test]
fn the_left_arrow_scrolls_the_line_left() {
    let mut a = app();
    a.hscroll = 5;
    act(&mut a, "<left>", Action::ScrollLeft);
    assert_eq!(a.hscroll, 4);
}

#[test]
fn l_scrolls_the_line_right() {
    let mut a = app();
    act(&mut a, "l", Action::ScrollRight);
    assert_eq!(a.hscroll, 1);
    assert_eq!(a.pane, Pane::Diff);
}

#[test]
fn the_right_arrow_scrolls_the_line_right() {
    let mut a = app();
    act(&mut a, "<right>", Action::ScrollRight);
    assert_eq!(a.hscroll, 1);
}

#[test]
fn scrolled_as_far_right_as_it_goes_the_longest_line_still_shows() {
    // The cap was put there because "a pane of blank columns reads as a
    // broken program", but the cap is the width itself, so at the cap the
    // longest line's last character has been scrolled off too.
    let mut a = app();
    a.tree_shown = false;
    a.rows.get_mut("src/a.rs").unwrap()[1].text = "fn main() { ENDMARK".into();
    assert!(
        draw(&mut a).contains("ENDMARK"),
        "the fixture draws the line"
    );
    for _ in 0..200 {
        press(&mut a, Key::Char('l'));
    }
    let screen = draw(&mut a);
    assert!(
        screen.contains('K'),
        "nothing of the line is left:\n{screen}"
    );
}

#[test]
fn w_is_forward_a_word() {
    let mut a = app();
    act(&mut a, "w", Action::WordForward);
    assert_eq!(a.hscroll, 3, "from `fn` to `main`");
}

#[test]
fn e_goes_to_the_end_of_the_first_word() {
    let mut a = app();
    act(&mut a, "e", Action::WordEnd);
    assert_eq!(a.hscroll, 1, "the `n` of `fn`");
}

#[test]
fn e_from_the_end_of_a_word_goes_to_the_end_of_the_next() {
    let mut a = app();
    a.hscroll = 1; // the `n` of `fn main() {`
    act(&mut a, "e", Action::WordEnd);
    assert_eq!(a.hscroll, 6, "the last `n` of `main`");
}

#[test]
fn b_is_back_a_word() {
    let mut a = app();
    a.hscroll = 5;
    act(&mut a, "b", Action::WordBack);
    assert_eq!(a.hscroll, 3, "the start of `main`");
}

#[test]
fn zero_is_the_start_of_the_line() {
    let mut a = app();
    a.hscroll = 7;
    act(&mut a, "0", Action::LineStart);
    assert_eq!(a.hscroll, 0);
}

#[test]
fn caret_is_the_first_word_of_the_line() {
    let mut a = app();
    a.cursor = 2; // "  old();"
    act(&mut a, "^", Action::FirstWord);
    assert_eq!(a.hscroll, 2);
}

#[test]
fn dollar_shows_the_end_of_the_line() {
    let mut a = app();
    a.tree_shown = false;
    a.rows.get_mut("src/a.rs").unwrap()[2].text = "  old_ZQX();".into();
    a.rows.get_mut("src/a.rs").unwrap()[4].text = format!("  {}", "x".repeat(60));
    a.cursor = 2;
    assert!(draw(&mut a).contains("ZQX"), "the fixture draws the line");
    act(&mut a, "$", Action::LineEnd);
    let screen = draw(&mut a);
    assert!(
        screen.contains("ZQX"),
        "the end of the cursor's line should be on screen:\n{screen}"
    );
}

#[test]
fn tab_is_the_next_pane() {
    let mut a = app();
    a.pane = Pane::Tree;
    act(&mut a, "<tab>", Action::PaneNext);
    assert_eq!(a.pane, Pane::Diff);
}

#[test]
fn shift_tab_is_the_previous_pane() {
    let mut a = app();
    act(&mut a, "<s-tab>", Action::PanePrev);
    assert_eq!(a.pane, Pane::Tree);
}

#[test]
fn pane_left_and_pane_right_walk_the_panes_when_a_keys_file_binds_them() {
    // Neither ships on a key, so neither is in the help; they are only
    // reachable through a keys file, and there they have to work.
    let mut a = app();
    a.keys = keys::Map::with("<C-h> = pane-left\n<C-k> = pane-right\n");
    a.on_key(Press::ctrl(Key::Char('h')));
    assert_eq!(a.pane, Pane::Tree);
    a.on_key(Press::ctrl(Key::Char('k')));
    assert_eq!(a.pane, Pane::Diff);
}

// --- search and modes ---

#[test]
fn slash_opens_the_search_over_every_file() {
    let mut a = app();
    let sent = recording(&mut a);
    act(&mut a, "/", Action::Search);
    assert_eq!(a.modal, Some(Modal::Finder));
    assert!(
        sent.borrow()
            .iter()
            .any(|r| matches!(r, Request::Corpus { .. })),
        "and every file's lines are asked for"
    );
}

/// The fixture with a corpus for the whole scope: `src/b.rs`, never opened,
/// says `needle()` on its second row.
fn with_corpus() -> App {
    let mut a = app();
    let b = vec![
        row(Kind::Header, None, None, "@@ -1 +1,2 @@"),
        row(Kind::Added, None, Some(1), "  needle();"),
    ];
    a.corpus
        .insert("src/a.rs".into(), a.rows["src/a.rs"].clone());
    a.corpus.insert("src/b.rs".into(), b);
    a.corpus_state = Load::Ready;
    a.corpus_for = Some((a.scope.clone(), a.opts()));
    a
}

#[test]
fn n_is_the_next_match_of_what_was_last_searched() {
    let mut a = with_corpus();
    act(&mut a, "/", Action::Search);
    typed(&mut a, "needle");
    press(&mut a, Key::Esc);
    act(&mut a, "n", Action::SearchNext);
    assert_eq!(a.path(), "src/b.rs");
}

#[test]
fn capital_n_is_the_previous_match() {
    let mut a = with_corpus();
    a.last_search = "new".into();
    a.cursor = 4;
    act(&mut a, "N", Action::SearchPrev);
    assert_eq!(a.path(), "src/a.rs");
    assert_eq!(a.cursor, 3, "`  new();`, above the cursor");
}

#[test]
fn v_is_visual_line_mode() {
    let mut a = app();
    act(&mut a, "v", Action::Visual);
    assert!(a.visual());
    assert_eq!(a.anchor, Some(1));
    act(&mut a, "v", Action::Visual);
    assert!(!a.visual(), "and the second press leaves it");
}

#[test]
fn capital_v_is_visual_line_mode() {
    let mut a = app();
    act(&mut a, "V", Action::Visual);
    assert!(a.visual());
}

#[test]
fn o_in_a_selection_goes_to_its_other_end() {
    let mut a = app();
    a.anchor = Some(1);
    a.cursor = 3;
    act(&mut a, "o", Action::OtherEnd);
    assert_eq!(a.cursor, 1);
    assert_eq!(a.anchor, Some(3));
}

#[test]
fn o_in_the_tree_folds_a_folder() {
    let mut a = app();
    a.pane = Pane::Tree;
    act(&mut a, "o", Action::OtherEnd);
    assert_eq!(a.tree_rows().len(), 1, "src/ folded");
    act(&mut a, "o", Action::OtherEnd);
    assert_eq!(a.tree_rows().len(), 3, "and opened");
}

#[test]
fn esc_cancels_the_selection() {
    let mut a = app();
    a.anchor = Some(3);
    act(&mut a, "<esc>", Action::Cancel);
    assert!(!a.visual());
}

#[test]
fn colon_opens_the_command_list() {
    let mut a = app();
    act(&mut a, ":", Action::Commands);
    assert_eq!(a.modal, Some(Modal::Palette));
    assert_eq!(a.palette_hits().len(), crate::input::COMMANDS.len());
}

#[test]
fn enter_on_a_line_opens_a_note_on_it() {
    let mut a = app();
    act(&mut a, "<cr>", Action::Enter);
    assert_eq!(a.modal, Some(Modal::Comment));
}

#[test]
fn enter_in_the_tree_opens_the_file_under_it() {
    let mut a = app();
    a.pane = Pane::Tree;
    a.goto_file(1);
    act(&mut a, "<cr>", Action::Enter);
    assert_eq!(a.pane, Pane::Diff, "into the pane");
    assert_eq!(a.path(), "src/b.rs");
}

#[test]
fn enter_in_the_queue_goes_to_the_lines_the_note_is_about() {
    let mut a = app();
    a.cursor = 3;
    queue_note(&mut a, "here");
    a.cursor = 1;
    a.queue_shown = true;
    a.pane = Pane::Queue;
    act(&mut a, "<cr>", Action::Enter);
    assert_eq!(a.pane, Pane::Diff);
    assert_eq!(a.cursor, 3);
}

#[test]
fn ctrl_l_repaints_the_screen() {
    let mut a = app();
    act(&mut a, "<C-l>", Action::Redraw);
    assert!(a.wants_redraw);
}

// --- commands, behind the leader ---

#[test]
fn leader_e_shows_and_hides_the_file_tree() {
    let mut a = app();
    act(&mut a, "<leader>e", Action::TreePane);
    assert!(a.tree_shown);
    assert_eq!(a.pane, Pane::Tree, "shown and not focused: go there");
    act(&mut a, "<leader>e", Action::TreePane);
    assert!(!a.tree_shown, "shown and focused: put it away");
    act(&mut a, "<leader>e", Action::TreePane);
    assert!(a.tree_shown, "and back");
}

#[test]
fn leader_c_shows_and_hides_the_review_queue() {
    let mut a = app();
    act(&mut a, "<leader>c", Action::QueuePane);
    assert!(a.queue_shown);
    assert_eq!(a.pane, Pane::Queue);
    act(&mut a, "<leader>c", Action::QueuePane);
    assert!(!a.queue_shown);
    assert_eq!(a.pane, Pane::Diff);
}

#[test]
fn leader_d_goes_back_to_the_code() {
    let mut a = app();
    a.pane = Pane::Tree;
    act(&mut a, "<leader>d", Action::CodePane);
    assert_eq!(a.pane, Pane::Diff);
}

#[test]
fn leader_l_opens_the_commit_history_and_asks_for_it() {
    let mut a = app();
    let sent = recording(&mut a);
    act(&mut a, "<leader>l", Action::History);
    assert_eq!(a.modal, Some(Modal::History));
    assert!(
        wants(&mut a, &sent)
            .iter()
            .any(|r| matches!(r, Request::Log { .. }))
    );
}

#[test]
fn leader_n_writes_a_note_on_the_selection() {
    let mut a = app();
    a.anchor = Some(2);
    a.cursor = 4;
    act(&mut a, "<leader>n", Action::Note);
    assert_eq!(a.modal, Some(Modal::Comment));
    typed(&mut a, "rename these");
    press(&mut a, Key::Enter);
    assert_eq!(a.comments.len(), 1);
    assert_eq!(a.comments[0].anchors.len(), 3, "every line of the range");
    assert_eq!(a.comments[0].body, "rename these");
    assert_eq!(a.toast, "comment queued · 3 lines");
}

#[test]
fn leader_x_deletes_the_note_under_the_cursor() {
    let mut a = app();
    queue_note(&mut a, "gone soon");
    act(&mut a, "<leader>x", Action::DeleteNote);
    assert!(a.comments.is_empty());
    assert_eq!(a.toast, "comment removed");
}

#[test]
fn leader_a_picks_the_target_agent() {
    let mut a = app();
    let sent = recording(&mut a);
    act(&mut a, "<leader>a", Action::Agents);
    assert_eq!(a.modal, Some(Modal::Agents));
    assert!(
        wants(&mut a, &sent)
            .iter()
            .any(|r| matches!(r, Request::Agents)),
        "a fresh list of who is free"
    );
    let kind = a.agent_choices()[0].0.clone();
    press(&mut a, Key::Enter);
    assert_eq!(a.new_kind.as_deref(), Some(kind.as_str()));
    assert_eq!(a.modal, None);
}

#[test]
fn leader_s_sends_the_queue_to_a_running_agent() {
    let mut a = app();
    let sent = recording(&mut a);
    a.agents = vec![idle_agent("claude")];
    queue_note(&mut a, "please look");
    act(&mut a, "<leader>s", Action::Send);
    let sent = sent.borrow();
    assert!(
        sent.iter().any(|r| matches!(
            r,
            Request::Send { pane, text } if pane == "w:claude" && text.contains("please look")
        )),
        "the queue went to the agent"
    );
    assert_eq!(a.comments[0].state, State::Sending);
}

#[test]
fn leader_s_starts_the_chosen_new_agent_with_the_queue() {
    let mut a = app();
    let sent = recording(&mut a);
    a.new_kind = Some("claude".into());
    queue_note(&mut a, "please look");
    act(&mut a, "<leader>s", Action::Send);
    assert!(sent.borrow().iter().any(|r| matches!(
        r,
        Request::Spawn { kind, text, .. } if kind == "claude" && text.contains("please look")
    )));
}

#[test]
fn leader_v_switches_between_split_and_unified() {
    let mut a = app();
    act(&mut a, "<leader>v", Action::Split);
    assert!(a.split);
    assert_eq!(a.toast, "split view");
    act(&mut a, "<leader>v", Action::Split);
    assert!(!a.split);
    assert_eq!(a.toast, "unified view");
}

#[test]
fn leader_b_turns_inline_blame_on_and_asks_for_it() {
    let mut a = app();
    let sent = recording(&mut a);
    act(&mut a, "<leader>b", Action::Blame);
    assert!(a.blame_on);
    assert!(
        wants(&mut a, &sent)
            .iter()
            .any(|r| matches!(r, Request::Blame { path, .. } if path == "src/a.rs"))
    );
    act(&mut a, "<leader>b", Action::Blame);
    assert!(!a.blame_on);
}

#[test]
fn leader_g_shows_the_blast_radius() {
    let mut a = app();
    act(&mut a, "<leader>g", Action::Deps);
    assert_eq!(a.modal, Some(Modal::Deps));
    let screen = draw(&mut a);
    assert!(screen.contains("BLAST RADIUS"), "{screen}");
    press(&mut a, Key::Char('x'));
    assert_eq!(a.modal, None, "any key puts it away");
}

#[test]
fn leader_plus_expands_the_context_and_asks_for_the_wider_diff() {
    let mut a = app();
    let sent = recording(&mut a);
    act(&mut a, "<leader>+", Action::ContextMore);
    assert_eq!(a.context, 6);
    assert!(
        wants(&mut a, &sent)
            .iter()
            .any(|r| matches!(r, Request::Diff { opts, .. } if opts.context == 6))
    );
}

#[test]
fn leader_equals_expands_the_context_too() {
    let mut a = app();
    act(&mut a, "<leader>=", Action::ContextMore);
    assert_eq!(a.context, 6);
}

#[test]
fn leader_minus_collapses_the_context() {
    let mut a = app();
    a.context = 9;
    act(&mut a, "<leader>-", Action::ContextLess);
    assert_eq!(a.context, 6);
}

#[test]
fn leader_r_refreshes_everything() {
    let mut a = app();
    let sent = recording(&mut a);
    act(&mut a, "<leader>r", Action::Refresh);
    assert_eq!(a.toast, "refreshing…");
    assert!(
        wants(&mut a, &sent)
            .iter()
            .any(|r| matches!(r, Request::Files { .. }))
    );
}

#[test]
fn leader_question_mark_is_this_help() {
    let mut a = app();
    act(&mut a, "<leader>?", Action::Help);
    assert_eq!(a.modal, Some(Modal::Help));
    let screen = draw(&mut a);
    assert!(screen.contains("KEYMAP"), "{screen}");
    assert!(screen.contains(Action::LineDown.about()), "{screen}");
}

#[test]
fn leader_t_picks_a_theme() {
    let mut a = app();
    act(&mut a, "<leader>t", Action::Themes);
    assert_eq!(a.modal, Some(Modal::Themes));
    let all = crate::tui::theme::Theme::all();
    assert_eq!(
        all.get(a.sel).copied(),
        Some(crate::tui::theme::current()),
        "it opens on the theme already in use"
    );
}

#[test]
fn leader_q_quits() {
    let mut a = app();
    act(&mut a, "<leader>q", Action::Quit);
    assert!(a.should_quit);
}

// --- the status bar ---
//
// "j/k move · }/]c hunk/change · ␣ leader · : commands · ␣? help", and in a
// selection "any motion extends · ␣n note on range · o other end · esc cancel".

#[test]
fn the_status_bar_says_j_and_k_move() {
    let mut a = app();
    press(&mut a, Key::Char('j'));
    assert_eq!(a.cursor, 2);
    press(&mut a, Key::Char('k'));
    assert_eq!(a.cursor, 1);
}

#[test]
fn the_status_bar_says_a_brace_is_a_hunk_and_bracket_c_a_change() {
    let mut a = two_hunks();
    press(&mut a, Key::Char('}'));
    assert_eq!(a.cursor, 6);
    let mut b = app();
    bracket(&mut b, ']', 'c');
    assert_eq!(b.cursor, 2);
}

#[test]
fn the_status_bar_says_space_is_the_leader() {
    let mut a = app();
    press(&mut a, Key::Char(' '));
    assert_eq!(a.pending, Pending::Leader);
    let screen = draw(&mut a);
    assert!(screen.contains('␣'), "the held leader is shown:\n{screen}");
}

#[test]
fn the_status_bar_says_colon_is_the_commands() {
    let mut a = app();
    press(&mut a, Key::Char(':'));
    assert_eq!(a.modal, Some(Modal::Palette));
}

#[test]
fn the_status_bar_says_leader_question_mark_is_help() {
    let mut a = app();
    press(&mut a, Key::Char(' '));
    press(&mut a, Key::Char('?'));
    assert_eq!(a.modal, Some(Modal::Help));
}

#[test]
fn in_a_selection_any_motion_extends_it() {
    for keys in ["j", "G", "}", "]c", "gj"] {
        let mut a = two_hunks();
        press(&mut a, Key::Char('V'));
        for c in keys.chars() {
            press(&mut a, Key::Char(c));
        }
        assert!(a.visual(), "{keys} left the selection");
        let (lo, hi) = a.span();
        assert_eq!(lo, 1, "{keys} moved the anchor");
        assert!(hi > lo, "{keys} did not extend it");
    }
}

#[test]
fn in_a_selection_leader_n_notes_the_whole_range() {
    let mut a = app();
    press(&mut a, Key::Char('V'));
    typed(&mut a, "jj");
    press(&mut a, Key::Char(' '));
    press(&mut a, Key::Char('n'));
    typed(&mut a, "all three");
    press(&mut a, Key::Enter);
    assert_eq!(a.comments[0].anchors.len(), 3);
    assert!(!a.visual(), "and the selection is spent");
}

#[test]
fn in_a_selection_o_goes_to_the_other_end() {
    let mut a = app();
    press(&mut a, Key::Char('V'));
    typed(&mut a, "jj");
    press(&mut a, Key::Char('o'));
    assert_eq!(a.cursor, 1);
    assert_eq!(a.span(), (1, 3));
}

#[test]
fn in_a_selection_esc_cancels_it() {
    let mut a = app();
    press(&mut a, Key::Char('V'));
    press(&mut a, Key::Char('j'));
    press(&mut a, Key::Esc);
    assert!(!a.visual());
}

// --- what the panes say ---

#[test]
fn the_queue_tab_says_leader_c_opens_the_queue() {
    // " no comments · ␣c ", hanging off the header while the queue is away.
    let mut a = app();
    let screen = draw(&mut a);
    assert!(screen.contains("no comments · ␣c"), "{screen}");
    press(&mut a, Key::Char(' '));
    press(&mut a, Key::Char('c'));
    assert!(a.queue_shown);
}

#[test]
fn the_empty_queue_says_v_first_takes_a_range() {
    let mut a = app();
    press(&mut a, Key::Char('V'));
    assert!(a.visual());
}

#[test]
fn the_empty_queue_names_the_key_that_writes_a_note_and_that_key_does() {
    let mut a = app();
    a.queue_shown = true;
    let screen = draw_wide(&mut a);
    assert!(screen.contains("press ␣n"), "{screen}");
    a.pane = Pane::Diff;
    press(&mut a, Key::Char(' '));
    press(&mut a, Key::Char('n'));
    assert_eq!(a.modal, Some(Modal::Comment));
}

#[test]
fn the_queue_footer_names_the_key_that_picks_an_agent_and_that_key_does() {
    let mut a = app();
    a.queue_shown = true;
    let screen = draw_wide(&mut a);
    assert!(screen.contains("␣a picks one"), "{screen}");
    press(&mut a, Key::Char(' '));
    press(&mut a, Key::Char('a'));
    assert_eq!(a.modal, Some(Modal::Agents));
}

#[test]
fn the_queue_footer_names_the_key_that_sends_and_that_key_does() {
    let mut a = app();
    let sent = recording(&mut a);
    a.new_kind = Some("claude".into());
    queue_note(&mut a, "go");
    a.queue_shown = true;
    let screen = draw_wide(&mut a);
    assert!(screen.contains("␣s · send 1"), "{screen}");
    assert!(
        !screen.contains("⏎"),
        "enter in the queue goes to a note, not a send"
    );
    press(&mut a, Key::Char(' '));
    press(&mut a, Key::Char('s'));
    assert!(
        sent.borrow()
            .iter()
            .any(|r| matches!(r, Request::Spawn { .. }))
    );
}

#[test]
fn sending_to_an_agent_that_is_busy_says_leader_a_picks_another() {
    // "{kind}: {why} · ␣a to pick another" — and ␣a does open the picker.
    let mut a = app();
    let mut busy = idle_agent("claude");
    busy.focused = true;
    a.agents = vec![busy];
    queue_note(&mut a, "go");
    press(&mut a, Key::Char(' '));
    press(&mut a, Key::Char('s'));
    assert!(a.toast.contains("␣a to pick another"), "{}", a.toast);
    press(&mut a, Key::Char(' '));
    press(&mut a, Key::Char('a'));
    assert_eq!(a.modal, Some(Modal::Agents));
}

// --- the finder: "↑↓ move · ↵ jump · ⇥ scope · esc close" ---

#[test]
fn the_finder_moves_with_the_arrows() {
    let mut a = app();
    press(&mut a, Key::Char('/'));
    assert_eq!(a.hits().len(), 2);
    press(&mut a, Key::Down);
    assert_eq!(a.sel, 1);
    press(&mut a, Key::Up);
    assert_eq!(a.sel, 0);
}

#[test]
fn the_finder_jumps_to_what_it_found_on_enter() {
    let mut a = app();
    a.pane = Pane::Tree;
    press(&mut a, Key::Char('/'));
    typed(&mut a, "b.rs");
    press(&mut a, Key::Enter);
    assert_eq!(a.modal, None);
    assert_eq!(a.path(), "src/b.rs");
    assert_eq!(a.pane, Pane::Diff);
}

#[test]
fn the_finder_jumps_to_a_line_from_its_grep_tab() {
    let mut a = with_corpus();
    press(&mut a, Key::Char('/'));
    while a.finder_tab != FinderTab::Grep {
        press(&mut a, Key::Tab);
    }
    typed(&mut a, "more");
    press(&mut a, Key::Enter);
    assert_eq!(a.path(), "src/a.rs");
    assert_eq!(a.cursor, 4, "`  more();`");
}

#[test]
fn tab_in_the_finder_changes_what_it_searches() {
    let mut a = app();
    press(&mut a, Key::Char('/'));
    assert_eq!(a.finder_tab, FinderTab::Files);
    press(&mut a, Key::Tab);
    assert_eq!(a.finder_tab, FinderTab::Hunks);
}

#[test]
fn esc_closes_the_finder() {
    let mut a = app();
    press(&mut a, Key::Char('/'));
    typed(&mut a, "zz");
    press(&mut a, Key::Esc);
    assert_eq!(a.modal, None);
    assert!(a.query.is_empty());
    assert_eq!(a.path(), "src/a.rs", "and nothing was opened");
}

// --- the palette: "⏎ run · esc" ---

#[test]
fn enter_in_the_palette_runs_the_highlighted_command() {
    let mut a = app();
    press(&mut a, Key::Char(':'));
    typed(&mut a, "split or unified view");
    press(&mut a, Key::Enter);
    assert!(a.split);
    assert_eq!(a.modal, None);
}

#[test]
fn esc_closes_the_palette_without_running_anything() {
    let mut a = app();
    press(&mut a, Key::Char(':'));
    typed(&mut a, "quit");
    press(&mut a, Key::Esc);
    assert_eq!(a.modal, None);
    assert!(!a.should_quit);
}

// --- the theme picker: "⏎ keep · esc undo" ---

#[test]
fn enter_in_the_theme_picker_keeps_the_theme_and_writes_it_down() {
    // Enter on the theme already in use, so the global palette other tests
    // draw with is not changed under them.
    let mut a = app();
    let sent = recording(&mut a);
    press(&mut a, Key::Char(' '));
    press(&mut a, Key::Char('t'));
    let here = crate::tui::theme::Theme::all()[a.sel];
    press(&mut a, Key::Enter);
    assert_eq!(a.modal, None);
    assert_eq!(crate::tui::theme::current(), here);
    assert!(
        sent.borrow()
            .iter()
            .any(|r| matches!(r, Request::Write(Write::Theme(t)) if *t == here)),
        "the choice is remembered"
    );
}

#[test]
fn esc_in_the_theme_picker_undoes_the_preview() {
    use crate::tui::theme::{current, set};
    let mut a = app();
    press(&mut a, Key::Char(' '));
    press(&mut a, Key::Char('t'));
    let before = a.theme_before;
    press(&mut a, Key::Char('j'));
    press(&mut a, Key::Esc);
    let after = current();
    set(before);
    assert_eq!(after, before);
    assert_eq!(a.modal, None);
}

// --- the history: "⏎ review · esc" ---

#[test]
fn enter_in_the_history_reviews_that_commit() {
    let mut a = app();
    press(&mut a, Key::Char(' '));
    press(&mut a, Key::Char('l'));
    a.log = vec![commit('b', "newer"), commit('a', "fix the parser")];
    a.log_state = Load::Ready;
    typed(&mut a, "parser");
    press(&mut a, Key::Enter);
    assert_eq!(
        a.scope,
        Scope::Commit {
            sha: "a".repeat(40)
        }
    );
    assert!(a.toast.contains("fix the parser"), "{}", a.toast);
}

#[test]
fn esc_closes_the_history_and_the_review_stays_where_it_was() {
    let mut a = app();
    press(&mut a, Key::Char(' '));
    press(&mut a, Key::Char('l'));
    a.log = vec![commit('a', "one")];
    press(&mut a, Key::Esc);
    assert_eq!(a.modal, None);
    assert_eq!(a.scope, Scope::WorkingTree);
}

// --- the note editor: "↵ save to queue · esc discard" ---

#[test]
fn enter_in_the_note_editor_saves_it_to_the_queue() {
    let mut a = app();
    press(&mut a, Key::Enter);
    typed(&mut a, "why?");
    press(&mut a, Key::Enter);
    assert_eq!(a.comments.len(), 1);
    assert_eq!(a.comments[0].state, State::Queued);
    assert_eq!(a.modal, None);
}

#[test]
fn esc_in_the_note_editor_discards_it() {
    let mut a = app();
    press(&mut a, Key::Enter);
    typed(&mut a, "never mind");
    press(&mut a, Key::Esc);
    assert!(a.comments.is_empty());
    assert!(a.draft.is_empty());
    assert_eq!(a.modal, None);
}

// --- the help: " 1–n of m · j/k " ---

#[test]
fn j_and_k_scroll_the_help() {
    let mut a = app();
    press(&mut a, Key::Char(' '));
    press(&mut a, Key::Char('?'));
    press(&mut a, Key::Char('j'));
    assert_eq!(a.help_scroll, 1);
    press(&mut a, Key::Char('k'));
    assert_eq!(a.help_scroll, 0);
}

#[test]
fn the_help_lists_every_binding_with_what_it_does() {
    // Generated from the keymap, so a reader rebinding a key sees the new one.
    let a = app();
    let listing = a.keys.listing();
    assert_eq!(listing.len(), keys::DEFAULTS.len());
    for (spec, action) in keys::DEFAULTS {
        let shown = keys::parse_chord(spec).map(keys::Chord::spec);
        assert!(
            listing
                .iter()
                .any(|(s, a)| Some(s) == shown.as_ref() && a == action),
            "{spec} missing from the help"
        );
    }
}

// --- the agent picker ---

#[test]
fn the_agent_picker_moves_with_j_and_k_and_takes_a_running_agent() {
    let mut a = app();
    a.agents = vec![idle_agent("claude"), idle_agent("codex")];
    press(&mut a, Key::Char(' '));
    press(&mut a, Key::Char('a'));
    a.agents = vec![idle_agent("claude"), idle_agent("codex")];
    a.sel = 0;
    press(&mut a, Key::Char('j'));
    assert_eq!(a.sel, 1);
    press(&mut a, Key::Enter);
    assert_eq!(a.agent_idx, 1);
    assert_eq!(a.toast, "target → codex");
}

#[test]
fn the_agent_picker_offers_to_start_a_new_one_here() {
    // " start a new one here ", above one entry per kind that can be started.
    let mut a = app();
    press(&mut a, Key::Char(' '));
    press(&mut a, Key::Char('a'));
    let screen = draw(&mut a);
    assert!(screen.contains("start a new one here"), "{screen}");
    press(&mut a, Key::Enter);
    assert!(a.new_kind.is_some());
    assert!(a.toast.starts_with("target → a new "), "{}", a.toast);
}

// --- the palette's entries ---

#[test]
fn every_palette_entry_is_one_run_command_knows() {
    for (label, _) in crate::input::COMMANDS {
        let mut a = app();
        assert!(a.run_command(label), "{label} did nothing");
    }
}

#[test]
fn every_palette_entry_with_a_key_does_what_that_key_does() {
    // The palette shows each entry's key beside it, which is a promise that
    // the two are one command.
    for (label, action) in crate::input::COMMANDS {
        let Some(action) = action else { continue };
        let mut by_palette = app();
        let _ = by_palette.run_command(label);
        let mut by_key = app();
        by_key.run(*action, 1);
        assert_eq!(
            seen(&by_palette),
            seen(&by_key),
            "{label} is not {}",
            action.name()
        );
    }
}

#[test]
fn the_palette_toggles_blame() {
    let mut a = app();
    palette(&mut a, "toggle blame");
    assert!(a.blame_on);
    assert_eq!(a.toast, "blame on");
}

#[test]
fn the_palette_shows_the_blast_radius() {
    let mut a = app();
    palette(&mut a, "blast radius");
    assert_eq!(a.modal, Some(Modal::Deps));
}

#[test]
fn the_palette_expands_the_context() {
    let mut a = app();
    palette(&mut a, "expand context");
    assert_eq!(a.context, 6);
}

#[test]
fn the_palette_collapses_the_context() {
    let mut a = app();
    a.context = 9;
    palette(&mut a, "collapse context");
    assert_eq!(a.context, 6);
}

#[test]
fn the_palette_goes_to_the_next_file() {
    let mut a = app();
    palette(&mut a, "next file");
    assert_eq!(a.path(), "src/b.rs");
}

#[test]
fn the_palette_goes_to_the_previous_file() {
    let mut a = app();
    a.goto_file(1);
    palette(&mut a, "prev file");
    assert_eq!(a.path(), "src/a.rs");
}

#[test]
fn the_palette_adds_a_comment() {
    let mut a = app();
    palette(&mut a, "add comment");
    assert_eq!(a.modal, Some(Modal::Comment));
}

#[test]
fn the_palette_deletes_the_comment_under_the_cursor() {
    let mut a = app();
    queue_note(&mut a, "x");
    palette(&mut a, "delete comment under cursor");
    assert!(a.comments.is_empty());
}

#[test]
fn the_palette_picks_an_agent() {
    let mut a = app();
    palette(&mut a, "pick agent");
    assert_eq!(a.modal, Some(Modal::Agents));
}

#[test]
fn the_palette_sends_the_queue_to_the_agent() {
    let mut a = app();
    let sent = recording(&mut a);
    a.new_kind = Some("claude".into());
    queue_note(&mut a, "go");
    palette(&mut a, "send queue to agent");
    assert!(
        sent.borrow()
            .iter()
            .any(|r| matches!(r, Request::Spawn { .. }))
    );
}

#[test]
fn the_palette_clears_the_queue() {
    let mut a = app();
    queue_note(&mut a, "x");
    palette(&mut a, "clear queue");
    assert!(a.comments.is_empty());
    assert_eq!(a.toast, "queue cleared");
}

#[test]
fn the_palette_opens_the_commit_history() {
    let mut a = app();
    palette(&mut a, "commit history");
    assert_eq!(a.modal, Some(Modal::History));
}

#[test]
fn the_palette_goes_to_the_next_scope() {
    let mut a = three_scopes();
    palette(&mut a, "next scope");
    assert!(matches!(a.scope, Scope::Branch { .. }));
}

#[test]
fn the_palette_goes_to_the_previous_scope() {
    let mut a = three_scopes();
    palette(&mut a, "prev scope");
    assert!(matches!(a.scope, Scope::Commit { .. }));
}

#[test]
fn the_palette_switches_the_split_view() {
    let mut a = app();
    palette(&mut a, "split or unified view");
    assert!(a.split);
}

#[test]
fn the_palette_folds_all_folders() {
    let mut a = app();
    palette(&mut a, "fold or open all folders");
    assert_eq!(a.tree_rows().len(), 1);
}

#[test]
fn the_palette_ignores_whitespace() {
    let mut a = app();
    palette(&mut a, "ignore whitespace");
    assert!(a.ignore_ws);
}

#[test]
fn the_palette_toggles_the_file_tree() {
    let mut a = app();
    palette(&mut a, "toggle file tree");
    assert_eq!(a.pane, Pane::Tree, "shown already, so it goes there first");
    palette(&mut a, "toggle file tree");
    assert!(!a.tree_shown);
}

#[test]
fn the_palette_toggles_the_queue() {
    let mut a = app();
    palette(&mut a, "toggle queue");
    assert!(a.queue_shown);
}

#[test]
fn the_palette_finds() {
    let mut a = app();
    palette(&mut a, "find");
    assert_eq!(a.modal, Some(Modal::Finder));
}

#[test]
fn the_palette_picks_a_theme() {
    let mut a = app();
    palette(&mut a, "pick a theme");
    assert_eq!(a.modal, Some(Modal::Themes));
    press(&mut a, Key::Esc);
}

#[test]
fn the_palette_writes_a_keymap_to_start_from() {
    // With the settings in memory there is no config directory, and the
    // honest answer is to say so; where there is one, the template goes to
    // the worker to write. Which of the two depends on which store this
    // process settled on first, so either is accepted — but not silence.
    let mut a = app();
    let sent = recording(&mut a);
    palette(&mut a, "write a keymap to start from");
    let wrote = sent.borrow().iter().any(|r| {
        matches!(
            r,
            Request::Write(Write::File { path, text })
                if path.ends_with("keys") && *text == keys::template()
        )
    });
    assert!(wrote || a.toast == "no config directory", "{}", a.toast);
}

#[test]
fn the_palette_writes_a_theme_to_start_from() {
    let mut a = app();
    let sent = recording(&mut a);
    palette(&mut a, "write a theme to start from");
    assert!(sent.borrow().iter().any(|r| matches!(
        r,
        Request::Write(Write::File { path, .. }) if path.ends_with("mine.theme")
    )));
}

#[test]
fn the_palette_refreshes() {
    let mut a = app();
    let sent = recording(&mut a);
    palette(&mut a, "refresh");
    assert!(
        wants(&mut a, &sent)
            .iter()
            .any(|r| matches!(r, Request::Files { .. }))
    );
}

#[test]
fn the_palette_opens_the_help() {
    let mut a = app();
    palette(&mut a, "open help");
    assert_eq!(a.modal, Some(Modal::Help));
}

#[test]
fn the_palette_quits() {
    let mut a = app();
    palette(&mut a, "quit");
    assert!(a.should_quit);
}

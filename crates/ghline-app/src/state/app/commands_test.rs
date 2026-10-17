//! One test per advertised command: every key in `HELP`, every hint the
//! status bar shows for a pane, every `:` command `run_cmd` knows and every
//! key a modal's own hint names. Each test is named for the promise and
//! checks the promise, pressed as a reader would press it.
//!
//! Kept apart from `tests.rs` because it is organised by what is advertised
//! rather than by what the code is made of: when a hint changes, this is the
//! file that says whether the key still does what the hint now claims.

use super::*;
use crate::actions::Prompt;
use crate::data::{DiffKind, MergeMethod, Status, Verdict};
use crate::shared::key::{Key, Press};
use crate::shared::mux::{Agent, AgentStatus};

// --------------------------------------------------------------- helpers

fn seeded() -> App {
    // Settings go to memory, never to the reader's own config.
    let _ = crate::shared::settings::use_store(Box::<crate::shared::settings::Memory>::default());
    crate::fixture::app()
}

/// A worker that answers nothing and keeps every request it was handed.
#[derive(Default)]
struct Recorder(std::cell::RefCell<Vec<Request>>);

impl Worker<Request, Response> for std::rc::Rc<Recorder> {
    fn send(&self, req: Request) -> bool {
        self.0.borrow_mut().push(req);
        true
    }

    fn poll(&self) -> Result<Option<Response>, crate::shared::worker::Gone> {
        Ok(None)
    }
}

fn recording() -> (App, std::rc::Rc<Recorder>) {
    let tape = std::rc::Rc::new(Recorder::default());
    let mut app = seeded();
    app.service = Some(Box::new(std::rc::Rc::clone(&tape)));
    (app, tape)
}

fn with_sidebar() -> App {
    let mut app = seeded();
    app.sidebar = true;
    app.sidebar_shown = true;
    app
}

fn press(app: &mut App, key: Key) {
    app.on_key(Press::new(key));
}

fn ch(app: &mut App, c: char) {
    press(app, Key::Char(c));
}

fn ctrl(app: &mut App, c: char) {
    app.on_key(Press::ctrl(Key::Char(c)));
}

fn typed(app: &mut App, text: &str) {
    for c in text.chars() {
        ch(app, c);
    }
}

/// `:` and a command and enter, as typed.
fn command(app: &mut App, text: &str) {
    ch(app, ':');
    typed(app, text);
    press(app, Key::Enter);
}

fn render(app: &mut App) {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();
    term.draw(|f| crate::ui::draw(f, app)).unwrap();
}

fn region(app: &App, target: hit::Target) -> hit::Region {
    app.hits
        .iter()
        .rev()
        .find(|r| r.target == target)
        .copied()
        .unwrap_or_else(|| panic!("the last frame drew no {target:?}"))
}

fn mouse(app: &mut App, what: crate::shared::key::Motion, col: u16, row: u16) {
    app.on_mouse(crate::shared::key::Mouse { col, row, what });
}

fn left() -> crate::shared::key::Motion {
    crate::shared::key::Motion::Down(crate::shared::key::Button::Left)
}

/// The fixture's first pull request, opened: body focused, checks beside.
fn pr_detail() -> App {
    let mut app = seeded();
    ch(&mut app, '2');
    press(&mut app, Key::Enter);
    assert_eq!(app.view, View::Detail);
    app
}

/// The same, with the checks a pull request has in real use. The fixture's
/// pull requests carry no run id, so their run is filed under 0.
fn pr_detail_with_checks() -> App {
    let mut app = pr_detail();
    let key = app.item_repo_key();
    app.jobs_by_run
        .insert((key.clone(), 0), crate::fixture::jobs());
    app.jobs_state.insert((key, 0), Load::Ready);
    app
}

/// The first pull request's diff, its file tree focused.
fn in_the_diff() -> App {
    let mut app = seeded();
    ch(&mut app, '2');
    ch(&mut app, 'd');
    assert_eq!(app.view, View::Diff);
    assert_eq!(app.pane, Pane::Files);
    app
}

/// The same, with the diff pane focused.
fn in_the_diff_body() -> App {
    let mut app = in_the_diff();
    press(&mut app, Key::Enter);
    assert_eq!(app.pane, Pane::DiffBody);
    app
}

/// A workflow run's logs, its jobs tree focused.
fn in_the_logs() -> App {
    let mut app = seeded();
    ch(&mut app, '3');
    press(&mut app, Key::Enter);
    ch(&mut app, 'l');
    press(&mut app, Key::Enter);
    assert_eq!(app.view, View::Logs);
    assert_eq!(app.pane, Pane::Tree);
    app
}

fn agent(kind: &str, title: &str, pane: &str) -> Agent {
    Agent {
        kind: kind.into(),
        status: AgentStatus::Idle,
        cwd: "/home/x/code/tuikit".into(),
        pane: pane.into(),
        title: title.into(),
        focused: false,
    }
}

fn with_agents() -> App {
    let mut app = seeded();
    app.agents = vec![
        agent("claude", "fixing the parser", "wA:p1"),
        agent("codex", "writing docs", "wA:p2"),
    ];
    app.agents_state = Load::Ready;
    app
}

fn entry(path: &str, is_dir: bool) -> crate::data::TreeEntry {
    crate::data::TreeEntry {
        path: path.into(),
        is_dir,
        size: 100,
    }
}

/// The Files tab over a small tree, with a checkout of it on disk: this
/// crate's own directory, so the paths `E` checks are real.
fn in_the_explorer() -> App {
    let mut app = seeded();
    ch(&mut app, '4');
    assert_eq!(app.pane, Pane::FileTree);
    let key = app.repo_key();
    app.trees.insert(
        key.clone(),
        vec![
            entry("Cargo.toml", false),
            entry("src", true),
            entry("src/lib.rs", false),
            entry("src/data.rs", false),
        ],
    );
    app.trees_state.insert(key.clone(), Load::Ready);
    app.file_text.insert(
        (key.clone(), "Cargo.toml".into()),
        "[package]\nname = \"x\"\nversion = \"0\"\nedition = \"2024\"\n".into(),
    );
    app.clones_state = Load::Ready;
    app.clones
        .insert(key, std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    app
}

/// What the diff pane draws for the open file.
fn diff_rows(app: &mut App) -> Vec<DiffKind> {
    app.prepared().rows.iter().map(|r| r.kind).collect()
}

/// The first added line of the open file.
fn an_added_line(app: &mut App) -> usize {
    diff_rows(app)
        .iter()
        .position(|k| *k == DiffKind::Add)
        .expect("the fixture's hunk adds lines")
}

fn is_dir_row(r: &files::Row) -> bool {
    matches!(r.entry, files::Entry::Dir(_))
}

// ------------------------------------------------------------- HELP: moving

#[test]
fn j_and_k_move_in_the_focused_pane() {
    let mut app = seeded();
    ch(&mut app, 'j');
    assert_eq!(app.item, 1);
    ch(&mut app, 'k');
    assert_eq!(app.item, 0);
}

#[test]
fn h_and_l_move_focus_one_pane_left_and_right() {
    let mut app = pr_detail();
    assert_eq!(app.pane, Pane::Body);
    ch(&mut app, 'l');
    assert_eq!(app.pane, Pane::Checks);
    ch(&mut app, 'h');
    assert_eq!(app.pane, Pane::Body);
}

#[test]
fn g_and_capital_g_go_to_the_top_and_the_bottom() {
    let mut app = seeded();
    ch(&mut app, 'G');
    assert_eq!(app.item, app.visible().len() - 1);
    ch(&mut app, 'g');
    assert_eq!(app.item, 0);
}

#[test]
fn one_two_and_three_show_issues_prs_and_actions() {
    let mut app = seeded();
    for (key, tab) in [('1', 0), ('2', 1), ('3', 2)] {
        ch(&mut app, key);
        assert_eq!(app.tab, tab);
        assert_eq!(app.view, View::List);
        assert_eq!(app.pane, Pane::List);
    }
}

#[test]
fn four_shows_the_files_of_the_repository() {
    let mut app = seeded();
    ch(&mut app, '4');
    assert_eq!(crate::data::TABS[app.tab].id, "files");
    assert_eq!(app.pane, Pane::FileTree);
}

#[test]
fn five_shows_the_agents_running_here() {
    let mut app = with_agents();
    ch(&mut app, '5');
    assert_eq!(crate::data::TABS[app.tab].id, "agents");
    assert_eq!(app.pane, Pane::Agents);
    assert_eq!(
        app.agents_visible().len(),
        2,
        "both work in this repository"
    );
}

#[test]
fn enter_goes_into_the_focused_pane() {
    let mut app = seeded();
    press(&mut app, Key::Enter);
    assert_eq!(app.view, View::Detail);
    assert_eq!(app.pane, Pane::Body);
}

#[test]
fn esc_and_q_each_go_back_one_level() {
    let mut app = pr_detail();
    press(&mut app, Key::Esc);
    assert_eq!(app.view, View::List);

    let mut app = pr_detail();
    ch(&mut app, 'q');
    assert_eq!(app.view, View::List);
    assert_eq!(app.pane, Pane::List);
}

#[test]
fn tab_and_shift_tab_move_to_the_next_and_previous_pane() {
    let mut app = pr_detail();
    press(&mut app, Key::Tab);
    assert_eq!(app.pane, Pane::Checks);
    press(&mut app, Key::BackTab);
    assert_eq!(app.pane, Pane::Body);
}

#[test]
fn ctrl_d_and_ctrl_u_scroll_half_a_page() {
    let mut app = pr_detail();
    app.detail_height = 20;
    ctrl(&mut app, 'd');
    assert_eq!(app.detail_scroll, 10);
    ctrl(&mut app, 'u');
    assert_eq!(app.detail_scroll, 0);
}

#[test]
fn page_down_and_page_up_scroll_a_whole_page() {
    let mut app = pr_detail();
    app.detail_height = 20;
    press(&mut app, Key::PageDown);
    assert_eq!(app.detail_scroll, 20);
    press(&mut app, Key::PageUp);
    assert_eq!(app.detail_scroll, 0);
}

#[test]
fn page_down_on_a_list_moves_twice_as_far_as_half_a_page() {
    let mut app = seeded();
    app.list_rows = 4;
    ctrl(&mut app, 'd');
    let half = app.item;
    app.item = 0;
    press(&mut app, Key::PageDown);
    assert!(half > 0);
    assert_eq!(app.item, half * 2, "a page is two half pages");
}

#[test]
fn ctrl_c_quits() {
    let mut app = seeded();
    ctrl(&mut app, 'c');
    assert!(app.should_quit);
}

#[test]
fn ctrl_l_asks_for_the_screen_to_be_repainted() {
    let mut app = seeded();
    ctrl(&mut app, 'l');
    assert!(app.wants_redraw);
    assert_eq!(app.pane, Pane::List, "and is not a plain l");
}

// ---------------------------------------------- HELP: accounts, themes, repos

#[test]
fn a_switches_account() {
    let mut app = seeded();
    ch(&mut app, 'a');
    assert!(app.accounts_open);
    ch(&mut app, 'j');
    press(&mut app, Key::Enter);
    assert!(!app.accounts_open);
    assert_eq!(app.acc, 1);
    assert_eq!(app.login(), "acme-platform");
}

#[test]
fn esc_leaves_the_account_switcher_without_switching() {
    let mut app = seeded();
    ch(&mut app, 'a');
    ch(&mut app, 'j');
    press(&mut app, Key::Esc);
    assert!(!app.accounts_open);
    assert_eq!(app.acc, 0);
}

#[test]
fn t_opens_the_theme_picker_and_j_previews_the_next_theme() {
    let mut app = seeded();
    let before = crate::tui::theme::current();
    ch(&mut app, 't');
    assert!(app.themes_open);
    ch(&mut app, 'j');
    assert_ne!(crate::tui::theme::current(), before, "j previews");
    press(&mut app, Key::Esc);
    assert!(!app.themes_open);
    assert_eq!(crate::tui::theme::current(), before, "esc puts it back");
}

#[test]
fn enter_in_the_theme_picker_keeps_the_previewed_theme() {
    let mut app = seeded();
    let before = crate::tui::theme::current();
    ch(&mut app, 't');
    ch(&mut app, 'j');
    let chosen = crate::tui::theme::current();
    press(&mut app, Key::Enter);
    assert!(!app.themes_open);
    assert_eq!(crate::tui::theme::current(), chosen);
    crate::tui::theme::set(before);
}

#[test]
fn b_hides_and_shows_the_repositories() {
    let mut app = seeded();
    assert!(!app.sidebar);
    ch(&mut app, 'b');
    assert!(app.sidebar);
    ch(&mut app, 'b');
    assert!(!app.sidebar);
}

#[test]
fn ctrl_b_hides_and_shows_the_repositories_too() {
    let mut app = seeded();
    ctrl(&mut app, 'b');
    assert!(app.sidebar);
}

#[test]
fn brackets_go_to_the_previous_and_next_repository() {
    let mut app = seeded();
    let at = app.repo;
    ch(&mut app, ']');
    assert_eq!(app.repo, at + 1);
    ch(&mut app, '[');
    assert_eq!(app.repo, at);
}

#[test]
fn p_finds_repos_issues_and_prs() {
    let mut app = seeded();
    ch(&mut app, 'p');
    assert!(app.finder_open);
    assert_eq!(app.finder_source, crate::data::Source::Repos);
}

#[test]
fn ctrl_p_opens_the_finder_too() {
    let mut app = seeded();
    ctrl(&mut app, 'p');
    assert!(app.finder_open);
}

#[test]
fn question_mark_shows_this_help_and_again_hides_it() {
    let mut app = seeded();
    ch(&mut app, '?');
    assert!(app.help_open);
    ch(&mut app, '?');
    assert!(!app.help_open);
}

#[test]
fn colon_opens_the_command_line() {
    let mut app = seeded();
    ch(&mut app, ':');
    assert_eq!(app.cmd, Some(Cmd::Colon));
}

#[test]
fn slash_filters_the_list() {
    let mut app = seeded();
    ch(&mut app, '/');
    typed(&mut app, "finder");
    press(&mut app, Key::Enter);
    let titles: Vec<&str> = app
        .visible()
        .iter()
        .map(|&i| app.list()[i].title.as_str())
        .collect();
    assert!(!titles.is_empty());
    assert!(titles.iter().all(|t| t.contains("finder")), "{titles:?}");
}

#[test]
fn slash_in_the_diff_searches_it() {
    let mut app = in_the_diff_body();
    ch(&mut app, '/');
    typed(&mut app, "debug_assert");
    press(&mut app, Key::Enter);
    let hit = app.diff_hit.expect("a match");
    let text = app.prepared().rows[hit.row].text.clone();
    assert!(text.contains("debug_assert"), "{text}");
    assert_eq!(app.diff_cursor, hit.row, "the cursor is on the match");
}

#[test]
fn r_refreshes_by_asking_github_again() {
    let (mut app, tape) = recording();
    ch(&mut app, 'r');
    assert_eq!(
        app.lists_state[&(app.repo_key(), 1)],
        Load::Idle,
        "the list is marked to be fetched again"
    );
    for _ in 0..3 {
        app.ensure();
    }
    let sent = tape.0.borrow();
    assert!(
        sent.iter()
            .any(|r| matches!(r, Request::List { tab: 1, repo } if *repo == app.repo_key())),
        "and it is asked for"
    );
}

// ------------------------------------------------------------ HELP: logs

#[test]
fn o_folds_a_job_in_the_logs() {
    let mut app = in_the_logs();
    let before = app.flat_tree().len();
    ch(&mut app, 'o');
    assert!(app.collapsed.contains(&0));
    assert!(app.flat_tree().len() < before, "its steps are hidden");
    ch(&mut app, 'o');
    assert_eq!(app.flat_tree().len(), before, "and o opens it again");
}

#[test]
fn capital_o_folds_every_job_of_the_logs_tree() {
    let mut app = in_the_logs();
    ch(&mut app, 'O');
    assert_eq!(app.collapsed.len(), app.jobs().len());
}

#[test]
fn e_jumps_to_the_first_error_of_the_log() {
    let mut app = in_the_logs();
    // the failing job, whose log has errors in it
    app.tree_sel = app
        .flat_tree()
        .iter()
        .position(|n| n.kind == NodeKind::Job && n.name == "test")
        .unwrap();
    app.log_scroll = 50;
    ch(&mut app, 'e');
    let first = app
        .log_lines()
        .iter()
        .position(|l| l.kind == crate::data::LogKind::Error)
        .unwrap();
    assert!(app.log_scroll <= first && first - app.log_scroll <= 3);
    assert!(!app.follow);
}

#[test]
fn f_toggles_following_the_log() {
    let mut app = in_the_logs();
    assert!(app.follow);
    ch(&mut app, 'f');
    assert!(!app.follow);
    ch(&mut app, 'f');
    assert!(app.follow);
}

// ------------------------------------------------------------ HELP: diff

#[test]
fn d_opens_the_diff_of_the_pr_files() {
    let mut app = seeded();
    ch(&mut app, 'd');
    assert_eq!(app.view, View::Diff);
    assert!(!app.diff_files().is_empty());
}

#[test]
fn s_switches_between_split_and_unified() {
    let mut app = in_the_diff_body();
    ch(&mut app, 's');
    assert!(app.split);
    ch(&mut app, 's');
    assert!(!app.split);
}

#[test]
fn w_ignores_whitespace() {
    let mut app = in_the_diff_body();
    ch(&mut app, 'w');
    assert!(app.ws);
    ch(&mut app, 'w');
    assert!(!app.ws);
}

#[test]
fn braces_go_to_the_previous_and_next_hunk() {
    let mut app = in_the_diff_body();
    let first = app.file_idx;
    // inside the hunk, `{` goes back to its header
    app.diff_cursor = 3;
    ch(&mut app, '{');
    assert_eq!(app.diff_cursor, 0);
    // the fixture's files have one hunk each, so the next is the next file's
    ch(&mut app, '}');
    assert_ne!(app.file_idx, first);
    assert_eq!(diff_rows(&mut app)[app.diff_cursor], DiffKind::Hdr);
    ch(&mut app, '{');
    assert_eq!(app.file_idx, first, "and back");
}

#[test]
fn capital_j_and_k_go_to_the_next_and_previous_file() {
    let mut app = in_the_diff_body();
    let first = app.file_idx;
    ch(&mut app, 'J');
    assert_ne!(app.file_idx, first);
    ch(&mut app, 'K');
    assert_eq!(app.file_idx, first);
}

#[test]
fn capital_h_and_l_pan_the_diff() {
    let mut app = in_the_diff_body();
    ch(&mut app, 'L');
    assert!(app.diff_pan > 0);
    ch(&mut app, 'H');
    assert_eq!(app.diff_pan, 0);
}

#[test]
fn n_and_capital_n_go_to_the_next_and_previous_match() {
    let mut app = in_the_diff_body();
    ch(&mut app, '/');
    typed(&mut app, "debug_assert");
    press(&mut app, Key::Enter);
    let first = app.diff_hit.unwrap();
    ch(&mut app, 'n');
    let second = app.diff_hit.unwrap();
    assert_ne!(first, second, "n moves on");
    ch(&mut app, 'N');
    assert_eq!(app.diff_hit.unwrap(), first, "N comes back");
}

#[test]
fn parens_go_to_the_previous_and_next_comment() {
    let mut app = in_the_diff_body();
    ch(&mut app, ')');
    let first = (app.file_idx, app.diff_cursor);
    assert_eq!(diff_rows(&mut app)[app.diff_cursor], DiffKind::NoteHead);
    ch(&mut app, ')');
    assert_ne!((app.file_idx, app.diff_cursor), first);
    assert_eq!(diff_rows(&mut app)[app.diff_cursor], DiffKind::NoteHead);
    ch(&mut app, '(');
    assert_eq!((app.file_idx, app.diff_cursor), first);
}

#[test]
fn capital_c_shows_and_hides_comments() {
    let mut app = in_the_diff_body();
    assert!(app.diff_notes);
    ch(&mut app, 'C');
    assert!(!app.diff_notes);
    ch(&mut app, 'C');
    assert!(app.diff_notes);
}

#[test]
fn v_picks_one_commit_of_the_pr() {
    let mut app = in_the_diff_body();
    ch(&mut app, 'v');
    assert!(app.commits_open);
    ch(&mut app, 'j');
    press(&mut app, Key::Enter);
    assert!(!app.commits_open);
    assert_eq!(app.diff_commit.as_deref(), Some("4f1c2a9e0b7d"));
}

#[test]
fn i_writes_a_note_on_this_line() {
    let mut app = in_the_diff_body();
    app.diff_cursor = an_added_line(&mut app);
    ch(&mut app, 'i');
    assert!(app.note_open.is_some());
    typed(&mut app, "why");
    press(&mut app, Key::Enter);
    assert!(app.note_open.is_none());
    assert_eq!(app.drafts().len(), 1);
    assert_eq!(app.drafts()[0].body, "why");
}

#[test]
fn ctrl_j_is_a_new_line_in_a_note() {
    let mut app = in_the_diff_body();
    app.diff_cursor = an_added_line(&mut app);
    ch(&mut app, 'i');
    typed(&mut app, "one");
    ctrl(&mut app, 'j');
    typed(&mut app, "two");
    press(&mut app, Key::Enter);
    assert_eq!(app.drafts()[0].body, "one\ntwo");
}

#[test]
fn esc_in_a_note_throws_it_away() {
    let mut app = in_the_diff_body();
    app.diff_cursor = an_added_line(&mut app);
    ch(&mut app, 'i');
    typed(&mut app, "nope");
    press(&mut app, Key::Esc);
    assert!(app.note_open.is_none());
    assert!(app.drafts().is_empty());
}

#[test]
fn capital_x_drops_the_note_here() {
    let mut app = in_the_diff_body();
    app.diff_cursor = an_added_line(&mut app);
    ch(&mut app, 'i');
    typed(&mut app, "why");
    press(&mut app, Key::Enter);
    assert_eq!(app.drafts().len(), 1);
    ch(&mut app, 'X');
    assert!(app.drafts().is_empty());
}

#[test]
fn capital_r_sends_the_review() {
    let (mut app, tape) = recording();
    ch(&mut app, '2');
    ch(&mut app, 'd');
    press(&mut app, Key::Enter);
    let num = app.current().unwrap().num;
    app.diff_cursor = an_added_line(&mut app);
    ch(&mut app, 'i');
    typed(&mut app, "a constant?");
    press(&mut app, Key::Enter);
    tape.0.borrow_mut().clear();

    ch(&mut app, 'R');
    assert!(app.review_open);
    press(&mut app, Key::Tab); // Comment -> Approve
    typed(&mut app, "lgtm");
    ctrl(&mut app, 'j');
    typed(&mut app, "thanks");
    press(&mut app, Key::Enter);

    assert!(!app.review_open);
    let sent = tape.0.borrow();
    assert!(
        sent.iter().any(|r| matches!(
            r,
            Request::SubmitReview { num: n, verdict: Verdict::Approve, body, drafts, .. }
                if *n == num && body == "lgtm\nthanks" && drafts.len() == 1
        )),
        "the verdict, the body with its new line, and the draft go together"
    );
}

#[test]
fn esc_in_the_review_dialog_sends_nothing() {
    let (mut app, tape) = recording();
    ch(&mut app, '2');
    ch(&mut app, 'd');
    tape.0.borrow_mut().clear();
    ch(&mut app, 'R');
    typed(&mut app, "lgtm");
    press(&mut app, Key::Esc);
    assert!(!app.review_open);
    assert!(
        !tape
            .0
            .borrow()
            .iter()
            .any(|r| matches!(r, Request::SubmitReview { .. }))
    );
}

#[test]
fn a_review_that_cannot_be_sent_is_not_sent() {
    let (mut app, tape) = recording();
    ch(&mut app, '2');
    ch(&mut app, 'd');
    tape.0.borrow_mut().clear();
    ch(&mut app, 'R');
    press(&mut app, Key::Enter); // a comment with nothing in it
    assert!(app.review_open, "it stays up, saying why");
    assert!(app.flash.is_some());
    assert!(tape.0.borrow().is_empty());
}

// ---------------------------------------------------- HELP: pull requests

#[test]
fn m_merges_the_pull_request_after_y() {
    let (mut app, tape) = recording();
    let cur = app.current().unwrap();
    let (num, branch) = (cur.num, cur.branch().to_string());
    ch(&mut app, 'm');
    assert_eq!(app.prompt, Some(Prompt::Merge(0)));
    ch(&mut app, 'y');
    assert!(app.prompt.is_none());
    assert!(tape.0.borrow().iter().any(|r| matches!(
        r,
        Request::Merge { num: n, method: MergeMethod::Merge, branch: b, .. }
            if *n == num && *b == branch
    )));
}

#[test]
fn one_two_three_and_j_k_choose_the_merge_method() {
    let (mut app, tape) = recording();
    ch(&mut app, 'm');
    ch(&mut app, '3');
    assert_eq!(app.prompt, Some(Prompt::Merge(2)));
    ch(&mut app, '1');
    assert_eq!(app.prompt, Some(Prompt::Merge(0)));
    ch(&mut app, 'j');
    assert_eq!(app.prompt, Some(Prompt::Merge(1)));
    ch(&mut app, 'j');
    ch(&mut app, 'k');
    assert_eq!(app.prompt, Some(Prompt::Merge(1)));
    ch(&mut app, '2');
    press(&mut app, Key::Enter);
    assert!(tape.0.borrow().iter().any(|r| matches!(
        r,
        Request::Merge {
            method: MergeMethod::Squash,
            ..
        }
    )));
}

#[test]
fn esc_and_n_cancel_a_merge_and_send_nothing() {
    for cancel in [Key::Esc, Key::Char('n')] {
        let (mut app, tape) = recording();
        ch(&mut app, 'm');
        press(&mut app, cancel);
        assert!(app.prompt.is_none());
        assert!(tape.0.borrow().is_empty(), "{cancel:?} sent something");
    }
}

#[test]
fn c_closes_an_open_pull_request_after_y() {
    let (mut app, tape) = recording();
    let num = app.current().unwrap().num;
    ch(&mut app, 'c');
    assert_eq!(app.prompt, Some(Prompt::Close));
    ch(&mut app, 'y');
    assert!(
        tape.0
            .borrow()
            .iter()
            .any(|r| matches!(r, Request::Close { num: n, .. } if *n == num))
    );
}

#[test]
fn once_github_says_it_is_closed_the_row_reads_closed_and_c_offers_to_reopen() {
    // live on Osmait/ghline#11: the all-repositories list is kept under its
    // own key, was not reloaded, and the PR went on reading as open
    let (mut app, _tape) = recording();
    let item = app.current().unwrap().clone();
    ch(&mut app, 'c');
    ch(&mut app, 'y');
    app.apply(Response::Action {
        repo: if item.repo.is_empty() {
            app.repo_key()
        } else {
            item.repo.clone()
        },
        num: item.num,
        result: Ok(format!("#{} closed", item.num)),
        merged_branch: None,
        now: Some(Status::Closed),
    });
    assert_eq!(app.current().unwrap().state, Status::Closed);
    ch(&mut app, 'c');
    assert_eq!(app.prompt, Some(Prompt::Reopen));
}

#[test]
fn c_reopens_a_closed_pull_request_after_y() {
    let (mut app, tape) = recording();
    app.item = app
        .visible()
        .iter()
        .position(|&i| app.list()[i].state == Status::Closed)
        .unwrap();
    let num = app.current().unwrap().num;
    ch(&mut app, 'c');
    assert_eq!(app.prompt, Some(Prompt::Reopen));
    ch(&mut app, 'y');
    assert!(
        tape.0
            .borrow()
            .iter()
            .any(|r| matches!(r, Request::Reopen { num: n, .. } if *n == num))
    );
}

#[test]
fn n_cancels_a_close_and_sends_nothing() {
    let (mut app, tape) = recording();
    ch(&mut app, 'c');
    ch(&mut app, 'n');
    assert!(app.prompt.is_none());
    assert!(tape.0.borrow().is_empty());
}

#[test]
fn capital_d_deletes_the_branch_of_a_merged_pull_request_after_y() {
    let (mut app, tape) = recording();
    app.item = app
        .visible()
        .iter()
        .position(|&i| app.list()[i].state == Status::Merged)
        .unwrap();
    let branch = app.current().unwrap().branch().to_string();
    ch(&mut app, 'D');
    assert!(matches!(app.prompt, Some(Prompt::DeleteBranch { .. })));
    ch(&mut app, 'y');
    assert!(
        tape.0
            .borrow()
            .iter()
            .any(|r| matches!(r, Request::DeleteBranch { branch: b, .. } if *b == branch))
    );
}

#[test]
fn esc_cancels_a_branch_deletion_and_sends_nothing() {
    let (mut app, tape) = recording();
    app.item = app
        .visible()
        .iter()
        .position(|&i| app.list()[i].state == Status::Merged)
        .unwrap();
    ch(&mut app, 'D');
    press(&mut app, Key::Esc);
    assert!(app.prompt.is_none());
    assert!(tape.0.borrow().is_empty());
}

#[test]
fn y_confirms_a_clone_and_esc_cancels_it() {
    let (mut app, tape) = recording();
    let ask = Prompt::Clone {
        repo: "ada-example/tuikit".into(),
        dest: "/tmp/code".into(),
    };
    app.prompt = Some(ask.clone());
    press(&mut app, Key::Esc);
    assert!(tape.0.borrow().is_empty());

    app.prompt = Some(ask);
    ch(&mut app, 'y');
    assert!(tape.0.borrow().iter().any(|r| matches!(
        r,
        Request::Clone { repo, dest } if repo == "ada-example/tuikit" && dest == "/tmp/code"
    )));
}

// ------------------------------------------------------- HELP: the mouse

#[test]
fn a_click_focuses_the_pane_and_selects_the_row() {
    let mut app = with_sidebar();
    app.pane = Pane::Repos;
    render(&mut app);
    let r = region(&app, hit::Target::Pane(Pane::List));
    mouse(&mut app, left(), r.area.x + 2, r.area.y + 2 * r.row_h);
    assert_eq!(app.pane, Pane::List);
    assert_eq!(app.item, 2);
}

#[test]
fn a_double_click_opens_it() {
    let mut app = seeded();
    render(&mut app);
    let r = region(&app, hit::Target::Pane(Pane::List));
    let (x, y) = (r.area.x + 2, r.area.y + r.row_h);
    let now = std::time::Instant::now();
    app.on_mouse_at(
        crate::shared::key::Mouse {
            col: x,
            row: y,
            what: left(),
        },
        now,
    );
    app.on_mouse_at(
        crate::shared::key::Mouse {
            col: x,
            row: y,
            what: left(),
        },
        now,
    );
    assert_eq!(app.view, View::Detail);
    assert_eq!(app.item, 1);
}

#[test]
fn the_wheel_scrolls_what_is_under_the_pointer() {
    let mut app = with_sidebar();
    app.pane = Pane::Repos;
    render(&mut app);
    let r = region(&app, hit::Target::Pane(Pane::List));
    mouse(
        &mut app,
        crate::shared::key::Motion::ScrollDown,
        r.area.x + 2,
        r.area.y + 1,
    );
    assert!(app.item > 0, "the list under the pointer moved");
    assert_eq!(app.pane, Pane::Repos, "without taking the focus");
}

// ---------------------------------------------- HELP: E and x, the agents

#[test]
fn capital_e_edits_the_file_in_the_editor() {
    let mut app = in_the_explorer();
    ch(&mut app, 'E');
    let (path, line) = app.edit_request.clone().expect("the editor is asked for");
    assert!(path.ends_with("Cargo.toml"), "{path:?}");
    assert_eq!(line, 1);
}

#[test]
fn x_sends_this_to_an_agent() {
    let (mut app, tape) = recording();
    app.agents = vec![agent("claude", "idle", "wA:p1")];
    app.agents_state = Load::Ready;
    let num = app.current().unwrap().num;
    ch(&mut app, 'x');
    assert!(app.dispatch_open);
    // the picker asks afresh which agents are free; answer as the worker would
    app.agents = vec![agent("claude", "idle", "wA:p1")];
    app.agents_state = Load::Ready;
    press(&mut app, Key::Enter);
    assert!(matches!(app.prompt, Some(Prompt::Dispatch { .. })));
    tape.0.borrow_mut().clear();
    ch(&mut app, 'y');
    assert!(tape.0.borrow().iter().any(|r| matches!(
        r,
        Request::Dispatch { pane, text } if pane == "wA:p1" && text.contains(&num.to_string())
    )));
}

#[test]
fn x_then_typing_says_something_specific() {
    let (mut app, tape) = recording();
    app.agents = vec![agent("claude", "idle", "wA:p1")];
    ch(&mut app, 'x');
    typed(&mut app, "only the parser");
    assert_eq!(app.dispatch_note, "only the parser");
    press(&mut app, Key::Enter);
    ch(&mut app, 'y');
    assert!(tape.0.borrow().iter().any(|r| matches!(
        r,
        Request::Dispatch { text, .. } if text.contains("only the parser")
    )));
}

#[test]
fn esc_cancels_a_dispatch_and_sends_nothing() {
    let (mut app, tape) = recording();
    app.agents = vec![agent("claude", "idle", "wA:p1")];
    ch(&mut app, 'x');
    press(&mut app, Key::Enter);
    assert!(app.prompt.is_some());
    tape.0.borrow_mut().clear();
    press(&mut app, Key::Esc);
    assert!(app.prompt.is_none());
    assert!(
        !tape
            .0
            .borrow()
            .iter()
            .any(|r| matches!(r, Request::Dispatch { .. } | Request::DispatchFresh { .. }))
    );
}

#[test]
fn esc_closes_the_dispatch_picker() {
    let mut app = with_agents();
    ch(&mut app, 'x');
    press(&mut app, Key::Esc);
    assert!(!app.dispatch_open);
    assert!(app.prompt.is_none());
}

#[test]
fn the_dispatch_picker_moves_on_the_arrows_and_ctrl_n_ctrl_p() {
    let mut app = with_agents();
    ch(&mut app, 'x');
    app.agents = with_agents().agents;
    press(&mut app, Key::Down);
    assert_eq!(app.dispatch_sel, 1);
    press(&mut app, Key::Up);
    assert_eq!(app.dispatch_sel, 0);
    ctrl(&mut app, 'n');
    assert_eq!(app.dispatch_sel, 1);
    ctrl(&mut app, 'p');
    assert_eq!(app.dispatch_sel, 0);
    assert!(app.dispatch_note.is_empty(), "moving is not typing");
}

#[test]
fn a_new_worktree_destination_starts_a_fresh_agent_after_y() {
    let (mut app, tape) = recording();
    let repo = app.item_repo_key();
    app.clones_state = Load::Ready;
    app.clones
        .insert(repo, std::path::PathBuf::from("/tmp/a-checkout"));
    let num = app.current().unwrap().num;
    ch(&mut app, 'x');
    assert!(matches!(
        app.dispatch_dests().first(),
        Some(Dest::Fresh { in_place: None, .. })
    ));
    press(&mut app, Key::Enter);
    ch(&mut app, 'y');
    let want = format!("issue-{num}");
    assert!(tape.0.borrow().iter().any(|r| matches!(
        r,
        Request::DispatchFresh { repo_root, branch: Some(b), .. }
            if repo_root == "/tmp/a-checkout" && *b == want
    )));
}

#[test]
fn the_checkout_destination_works_in_place_after_y() {
    let (mut app, tape) = recording();
    let repo = app.item_repo_key();
    app.clones_state = Load::Ready;
    app.clones
        .insert(repo, std::path::PathBuf::from("/tmp/a-checkout"));
    app.head_branches
        .insert("/tmp/a-checkout".into(), "main".into());
    ch(&mut app, 'x');
    let at = app
        .dispatch_dests()
        .iter()
        .position(|d| {
            matches!(
                d,
                Dest::Fresh {
                    in_place: Some(_),
                    ..
                }
            )
        })
        .expect("the checkout is offered");
    for _ in 0..at {
        press(&mut app, Key::Down);
    }
    press(&mut app, Key::Enter);
    ch(&mut app, 'y');
    assert!(
        tape.0
            .borrow()
            .iter()
            .any(|r| matches!(r, Request::DispatchFresh { branch: None, .. }))
    );
}

// ----------------------------------------------------------- `:` commands

#[test]
fn colon_account_opens_the_account_picker() {
    for word in ["account", "accounts"] {
        let mut app = seeded();
        command(&mut app, word);
        assert!(app.accounts_open, ":{word}");
    }
}

#[test]
fn colon_tab_names_jump_to_that_tab() {
    for (word, id, pane) in [
        ("issues", "issues", Pane::List),
        ("prs", "prs", Pane::List),
        ("actions", "actions", Pane::List),
        ("files", "files", Pane::FileTree),
        ("agents", "agents", Pane::Agents),
    ] {
        let mut app = pr_detail();
        command(&mut app, word);
        assert_eq!(crate::data::TABS[app.tab].id, id, ":{word}");
        assert_eq!(app.view, View::List, ":{word}");
        assert_eq!(app.pane, pane, ":{word}");
    }
}

#[test]
fn colon_logs_opens_the_logs_of_the_selection() {
    let mut app = seeded();
    ch(&mut app, '3');
    command(&mut app, "logs");
    assert_eq!(app.view, View::Logs);
    assert_eq!(app.pane, Pane::Tree);
    assert!(!app.flat_tree().is_empty(), "the selected run's jobs");
}

#[test]
fn colon_logs_on_an_issue_says_there_is_none_rather_than_opening_an_empty_view() {
    let mut app = seeded();
    command(&mut app, "logs");
    assert_eq!(app.view, View::List);
    assert!(app.flash.is_some());
}

#[test]
fn the_diffs_own_commands_outside_the_diff_say_where_they_work() {
    for cmd in ["split", "ws", "commits", "comments"] {
        let mut app = seeded();
        command(&mut app, cmd);
        assert_eq!(app.view, View::List, ":{cmd}");
        assert!(app.flash.is_some(), ":{cmd} said nothing");
    }
}

#[test]
fn colon_diff_opens_the_diff() {
    let mut app = seeded();
    command(&mut app, "diff");
    assert_eq!(app.view, View::Diff);
    assert_eq!(app.pane, Pane::Files);
}

#[test]
fn colon_review_opens_the_review_dialog() {
    let mut app = in_the_diff_body();
    command(&mut app, "review");
    assert!(app.review_open);

    // and from the list, on a pull request
    let mut app = seeded();
    command(&mut app, "review");
    assert!(app.review_open);
}

#[test]
fn colon_commits_opens_the_commit_picker() {
    let mut app = in_the_diff_body();
    command(&mut app, "commits");
    assert!(app.commits_open);
}

#[test]
fn colon_split_ws_and_comments_do_what_their_keys_do() {
    let mut app = in_the_diff_body();
    command(&mut app, "split");
    assert!(app.split);
    command(&mut app, "ws");
    assert!(app.ws);
    command(&mut app, "comments");
    assert!(!app.diff_notes);
}

#[test]
fn colon_send_opens_the_dispatch_picker() {
    let mut app = with_agents();
    command(&mut app, "send");
    assert!(app.dispatch_open);
}

#[test]
fn colon_refresh_refreshes() {
    let (mut app, _tape) = recording();
    app.lists_state.insert((app.repo_key(), 1), Load::Ready);
    command(&mut app, "refresh");
    assert_eq!(app.lists_state[&(app.repo_key(), 1)], Load::Idle);
}

#[test]
fn colon_theme_opens_the_theme_picker() {
    for word in ["theme", "themes"] {
        let mut app = seeded();
        command(&mut app, word);
        assert!(app.themes_open, ":{word}");
        press(&mut app, Key::Esc);
    }
}

#[test]
fn colon_sidebar_hides_and_shows_the_repositories() {
    for word in ["sidebar", "repos"] {
        let mut app = seeded();
        command(&mut app, word);
        assert!(app.sidebar, ":{word}");
        command(&mut app, word);
        assert!(!app.sidebar, ":{word}");
    }
}

#[test]
fn colon_find_opens_the_finder() {
    for word in ["find", "search"] {
        let mut app = seeded();
        command(&mut app, word);
        assert!(app.finder_open, ":{word}");
    }
}

#[test]
fn colon_help_opens_the_help() {
    for word in ["help", "h"] {
        let mut app = seeded();
        command(&mut app, word);
        assert!(app.help_open, ":{word}");
    }
}

#[test]
fn colon_q_goes_back_to_the_list() {
    for word in ["q", "quit"] {
        let mut app = pr_detail();
        command(&mut app, word);
        assert_eq!(app.view, View::List, ":{word}");
    }
}

#[test]
fn colon_q_from_the_diff_leaves_the_focus_on_the_list() {
    let mut app = in_the_diff_body();
    command(&mut app, "q");
    assert_eq!(app.view, View::List);
    assert!(
        app.panes().contains(&app.pane),
        "focus on {:?}, which the list does not draw",
        app.pane
    );
}

// --------------------------------------------------- the repositories pane

#[test]
fn on_the_repositories_j_and_k_change_repository() {
    let mut app = with_sidebar();
    app.pane = Pane::Repos;
    let at = app.repo;
    ch(&mut app, 'j');
    assert_eq!(app.repo, at + 1);
    ch(&mut app, 'k');
    assert_eq!(app.repo, at);
}

#[test]
fn on_the_repositories_l_moves_to_the_content() {
    let mut app = with_sidebar();
    app.pane = Pane::Repos;
    ch(&mut app, 'l');
    assert_eq!(app.pane, Pane::List);
}

#[test]
fn on_the_repositories_enter_opens_the_repository() {
    let mut app = with_sidebar();
    app.pane = Pane::Repos;
    ch(&mut app, 'j');
    press(&mut app, Key::Enter);
    assert_eq!(app.pane, Pane::List);
    assert_eq!(app.view, View::List);
}

#[test]
fn on_the_repositories_b_hides_them() {
    let mut app = with_sidebar();
    app.pane = Pane::Repos;
    ch(&mut app, 'b');
    assert!(!app.sidebar);
    assert_ne!(app.pane, Pane::Repos, "and the focus leaves");
}

#[test]
fn on_the_repositories_a_switches_account() {
    let mut app = with_sidebar();
    app.pane = Pane::Repos;
    ch(&mut app, 'a');
    assert!(app.accounts_open);
}

#[test]
fn on_the_repositories_colon_opens_the_command_line() {
    let mut app = with_sidebar();
    app.pane = Pane::Repos;
    ch(&mut app, ':');
    assert_eq!(app.cmd, Some(Cmd::Colon));
}

// ------------------------------------------------------------ the list pane

#[test]
fn on_the_list_h_goes_to_the_repositories_when_they_are_shown() {
    let mut app = with_sidebar();
    ch(&mut app, 'h');
    assert_eq!(app.pane, Pane::Repos);
}

#[test]
fn on_the_list_b_shows_the_repositories_when_they_are_hidden() {
    let mut app = seeded();
    ch(&mut app, 'b');
    assert!(app.sidebar);
}

#[test]
fn on_the_list_the_pr_keys_act_on_the_selected_pr() {
    let mut app = seeded();
    ch(&mut app, 'm');
    assert_eq!(app.prompt, Some(Prompt::Merge(0)));
    press(&mut app, Key::Esc);
    ch(&mut app, 'c');
    assert_eq!(app.prompt, Some(Prompt::Close));
    press(&mut app, Key::Esc);
    ch(&mut app, 'd');
    assert_eq!(app.view, View::Diff);
}

// ------------------------------------------------------------ the body pane

#[test]
fn on_the_body_j_and_k_scroll() {
    let mut app = pr_detail();
    ch(&mut app, 'j');
    ch(&mut app, 'j');
    assert_eq!(app.detail_scroll, 2);
    ch(&mut app, 'k');
    assert_eq!(app.detail_scroll, 1);
}

#[test]
fn on_the_body_h_goes_to_the_repositories_and_l_to_the_checks() {
    let mut app = with_sidebar();
    ch(&mut app, '2');
    press(&mut app, Key::Enter);
    ch(&mut app, 'l');
    assert_eq!(app.pane, Pane::Checks);
    ch(&mut app, 'h');
    ch(&mut app, 'h');
    assert_eq!(app.pane, Pane::Repos);
}

#[test]
fn on_the_body_esc_goes_back() {
    let mut app = pr_detail();
    press(&mut app, Key::Esc);
    assert_eq!(app.view, View::List);
}

#[test]
fn on_the_body_the_pr_keys_act_on_the_pr_shown() {
    let mut app = pr_detail();
    ch(&mut app, 'm');
    assert_eq!(app.prompt, Some(Prompt::Merge(0)));
    press(&mut app, Key::Esc);
    ch(&mut app, 'd');
    assert_eq!(app.view, View::Diff);
}

// ---------------------------------------------------------- the checks pane

#[test]
fn on_the_checks_j_and_k_move_between_checks() {
    let mut app = pr_detail_with_checks();
    ch(&mut app, 'l');
    ch(&mut app, 'j');
    assert_eq!(app.check, 1);
    ch(&mut app, 'k');
    assert_eq!(app.check, 0);
}

#[test]
fn on_the_checks_enter_opens_the_logs_of_that_check() {
    let mut app = pr_detail_with_checks();
    ch(&mut app, 'l');
    ch(&mut app, 'j');
    press(&mut app, Key::Enter);
    assert_eq!(app.view, View::Logs);
    let tree = app.flat_tree();
    assert_eq!(tree[app.tree_sel].ji, 1, "on the job that was selected");
}

#[test]
fn on_the_checks_h_goes_to_the_body_and_esc_back() {
    let mut app = pr_detail_with_checks();
    ch(&mut app, 'l');
    ch(&mut app, 'h');
    assert_eq!(app.pane, Pane::Body);
    ch(&mut app, 'l');
    press(&mut app, Key::Esc);
    assert_eq!(app.view, View::List);
}

#[test]
fn on_the_checks_the_pr_keys_act_on_the_pr_shown() {
    let mut app = pr_detail_with_checks();
    ch(&mut app, 'l');
    ch(&mut app, 'c');
    assert_eq!(app.prompt, Some(Prompt::Close));
}

// ---------------------------------------------------------- the agents pane

#[test]
fn on_the_agents_j_and_k_move_between_agents() {
    let mut app = with_agents();
    ch(&mut app, '5');
    ch(&mut app, 'j');
    assert_eq!(app.agent_idx(), 1);
    ch(&mut app, 'k');
    assert_eq!(app.agent_idx(), 0);
}

#[test]
fn on_the_agents_the_numbers_go_to_the_other_tabs() {
    let mut app = with_agents();
    ch(&mut app, '5');
    ch(&mut app, '1');
    assert_eq!(app.tab, 0);
    assert_eq!(app.pane, Pane::List);
}

#[test]
fn on_the_agents_slash_filters_them() {
    let mut app = with_agents();
    ch(&mut app, '5');
    ch(&mut app, '/');
    typed(&mut app, "parser");
    press(&mut app, Key::Enter);
    let shown: Vec<&str> = app
        .agents_visible()
        .iter()
        .map(|a| a.kind.as_str())
        .collect();
    assert_eq!(shown, ["claude"]);
}

#[test]
fn on_the_agents_r_asks_for_them_again() {
    let (mut app, tape) = recording();
    app.agents = with_agents().agents;
    ch(&mut app, '5');
    app.agents_state = Load::Ready;
    ch(&mut app, 'r');
    assert_eq!(app.agents_state, Load::Idle);
    tape.0.borrow_mut().clear();
    for _ in 0..3 {
        app.ensure();
    }
    assert!(tape.0.borrow().iter().any(|r| matches!(r, Request::Agents)));
}

// -------------------------------------------------------- the file explorer

#[test]
fn on_the_file_tree_j_and_k_move() {
    let mut app = in_the_explorer();
    ch(&mut app, 'j');
    assert_eq!(app.fs_idx(), 1);
    ch(&mut app, 'k');
    assert_eq!(app.fs_idx(), 0);
}

#[test]
fn on_the_file_tree_o_and_enter_fold_a_folder() {
    for key in [Key::Char('o'), Key::Enter] {
        let mut app = in_the_explorer();
        ch(&mut app, 'j'); // src
        let before = app.fs_rows().len();
        press(&mut app, key);
        assert!(app.fs_rows().len() > before, "{key:?} opens it");
        press(&mut app, key);
        assert_eq!(app.fs_rows().len(), before, "{key:?} folds it again");
    }
}

#[test]
fn capital_o_opens_every_folder_of_the_file_tree() {
    let mut app = in_the_explorer();
    ch(&mut app, 'O');
    assert_eq!(app.fs_rows().len(), app.repo_tree().len());
}

#[test]
fn on_the_file_tree_l_reads_the_file() {
    let mut app = in_the_explorer();
    ch(&mut app, 'l');
    assert_eq!(app.pane, Pane::FileView);
    assert!(app.file_body().is_ok());
}

#[test]
fn on_the_file_tree_slash_finds_a_file() {
    let mut app = in_the_explorer();
    ch(&mut app, '/');
    typed(&mut app, "data");
    press(&mut app, Key::Enter);
    let paths: Vec<&str> = app.fs_rows().iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, ["src/data.rs"], "even inside a folded folder");
}

#[test]
fn on_the_file_tree_x_sends_the_file() {
    let mut app = in_the_explorer();
    ch(&mut app, 'x');
    assert!(app.dispatch_open);
    assert_eq!(app.dispatch_subject(), Some(crate::subject::Subject::File));
}

#[test]
fn on_the_file_view_j_and_k_move_the_line() {
    let mut app = in_the_explorer();
    ch(&mut app, 'l');
    ch(&mut app, 'j');
    ch(&mut app, 'j');
    assert_eq!(app.file_sel, 2);
    ch(&mut app, 'k');
    assert_eq!(app.file_sel, 1);
}

#[test]
fn on_the_file_view_ctrl_d_and_ctrl_u_page() {
    let mut app = in_the_explorer();
    ch(&mut app, 'l');
    app.detail_height = 4;
    ctrl(&mut app, 'd');
    assert_eq!(app.file_sel, 2);
    ctrl(&mut app, 'u');
    assert_eq!(app.file_sel, 0);
}

#[test]
fn on_the_file_view_h_goes_back_to_the_files() {
    let mut app = in_the_explorer();
    ch(&mut app, 'l');
    ch(&mut app, 'h');
    assert_eq!(app.pane, Pane::FileTree);
}

#[test]
fn on_the_file_view_capital_e_edits_at_the_line_under_the_cursor() {
    let mut app = in_the_explorer();
    ch(&mut app, 'l');
    ch(&mut app, 'j');
    ch(&mut app, 'j');
    ch(&mut app, 'E');
    let (path, line) = app.edit_request.clone().expect("the editor is asked for");
    assert!(path.ends_with("Cargo.toml"));
    assert_eq!(line, 3, "the third line, counted from one");
}

#[test]
fn on_the_file_view_x_sends_the_file() {
    let mut app = in_the_explorer();
    ch(&mut app, 'l');
    ch(&mut app, 'x');
    assert!(app.dispatch_open);
    assert_eq!(app.dispatch_subject(), Some(crate::subject::Subject::File));
}

// ------------------------------------------------------- the logs, its panes

#[test]
fn on_the_jobs_tree_j_and_k_move_between_nodes() {
    let mut app = in_the_logs();
    let at = app.tree_sel;
    ch(&mut app, 'j');
    assert_eq!(app.tree_sel, at + 1);
    ch(&mut app, 'k');
    assert_eq!(app.tree_sel, at);
}

#[test]
fn on_the_jobs_tree_enter_and_l_go_to_the_log() {
    for key in [Key::Enter, Key::Char('l')] {
        let mut app = in_the_logs();
        press(&mut app, key);
        assert_eq!(app.pane, Pane::Log, "{key:?}");
    }
}

#[test]
fn on_the_jobs_tree_esc_goes_back() {
    let mut app = in_the_logs();
    press(&mut app, Key::Esc);
    assert_eq!(app.view, View::Detail);
}

#[test]
fn on_the_log_j_and_k_scroll() {
    let mut app = in_the_logs();
    press(&mut app, Key::Enter);
    ch(&mut app, 'j');
    ch(&mut app, 'j');
    assert_eq!(app.log_scroll, 2);
    ch(&mut app, 'k');
    assert_eq!(app.log_scroll, 1);
}

#[test]
fn on_the_log_slash_filters_its_lines() {
    let mut app = in_the_logs();
    press(&mut app, Key::Enter);
    app.tree_sel = 3; // the test job, all of it
    ch(&mut app, '/');
    typed(&mut app, "failed");
    press(&mut app, Key::Enter);
    let lines = app.log_lines();
    assert!(!lines.is_empty());
    assert!(
        lines
            .iter()
            .all(|l| l.text.to_lowercase().contains("failed"))
    );
}

#[test]
fn on_the_log_h_goes_to_the_jobs_and_esc_back() {
    let mut app = in_the_logs();
    press(&mut app, Key::Enter);
    ch(&mut app, 'h');
    assert_eq!(app.pane, Pane::Tree);
    press(&mut app, Key::Enter);
    press(&mut app, Key::Esc);
    assert_eq!(app.pane, Pane::Tree);
}

// ------------------------------------------------------- the diff's files

#[test]
fn on_the_changed_files_j_and_k_move() {
    let mut app = in_the_diff();
    let first = app.file_idx;
    ch(&mut app, 'j');
    ch(&mut app, 'j');
    assert_ne!(app.file_idx, first);
    ch(&mut app, 'k');
    ch(&mut app, 'k');
    assert_eq!(app.file_idx, first);
}

#[test]
fn on_the_changed_files_enter_opens_the_file() {
    let mut app = in_the_diff();
    press(&mut app, Key::Enter);
    assert_eq!(app.pane, Pane::DiffBody);
}

#[test]
fn on_the_changed_files_h_and_l_fold_and_open_a_folder() {
    let mut app = in_the_diff();
    let open = app.file_rows().len();
    ch(&mut app, 'h');
    assert!(app.file_rows().len() < open, "h folds the folder it is in");
    assert_eq!(app.pane, Pane::Files);
    ch(&mut app, 'l');
    assert_eq!(app.file_rows().len(), open, "l opens it again");
    assert_eq!(app.pane, Pane::Files);
}

#[test]
fn o_folds_the_folder_of_the_diffs_tree() {
    let mut app = in_the_diff();
    let open = app.file_rows().len();
    ch(&mut app, 'o');
    assert!(app.file_rows().len() < open);
    ch(&mut app, 'o');
    assert_eq!(app.file_rows().len(), open);
}

#[test]
fn capital_o_folds_every_folder_of_the_diffs_tree_and_again_opens_them() {
    let mut app = in_the_diff();
    let open = app.file_rows();
    ch(&mut app, 'O');
    let rows = app.file_rows();
    assert!(rows.iter().filter(|r| is_dir_row(r)).all(|r| r.folded));
    assert!(
        rows.iter().all(|r| r.depth == 0),
        "nothing under a folder shows"
    );
    ch(&mut app, 'O');
    assert_eq!(app.file_rows(), open);
}

#[test]
fn on_the_changed_files_v_s_and_w_work_too() {
    let mut app = in_the_diff();
    ch(&mut app, 's');
    assert!(app.split);
    ch(&mut app, 'w');
    assert!(app.ws);
    ch(&mut app, 'v');
    assert!(app.commits_open);
}

#[test]
fn on_the_changed_files_esc_goes_back() {
    let mut app = in_the_diff();
    press(&mut app, Key::Esc);
    assert_eq!(app.view, View::Detail);
}

#[test]
fn on_the_diff_j_and_k_move_the_line() {
    let mut app = in_the_diff_body();
    ch(&mut app, 'j');
    ch(&mut app, 'j');
    assert_eq!(app.diff_cursor, 2);
    ch(&mut app, 'k');
    assert_eq!(app.diff_cursor, 1);
}

// ------------------------------------------------------------- the modals

#[test]
fn in_the_finder_tab_and_shift_tab_change_the_source() {
    let mut app = seeded();
    ch(&mut app, 'p');
    press(&mut app, Key::Tab);
    assert_eq!(app.finder_source, crate::data::Source::Issues);
    press(&mut app, Key::BackTab);
    assert_eq!(app.finder_source, crate::data::Source::Repos);
}

#[test]
fn in_the_finder_the_arrows_and_ctrl_n_ctrl_p_move_the_row() {
    let mut app = seeded();
    ch(&mut app, 'p');
    assert!(app.finder_len() > 2);
    press(&mut app, Key::Down);
    assert_eq!(app.finder_sel, 1);
    ctrl(&mut app, 'n');
    assert_eq!(app.finder_sel, 2);
    ctrl(&mut app, 'p');
    press(&mut app, Key::Up);
    assert_eq!(app.finder_sel, 0);
    assert!(app.finder_query.is_empty(), "moving is not typing");
}

#[test]
fn in_the_finder_enter_opens_the_row() {
    let mut app = seeded();
    ch(&mut app, 'p');
    typed(&mut app, "dotfiles");
    press(&mut app, Key::Enter);
    assert!(!app.finder_open);
    assert_eq!(app.repo_name(), "dotfiles");
}

#[test]
fn in_the_finder_esc_closes_it() {
    let mut app = seeded();
    let repo = app.repo;
    ch(&mut app, 'p');
    typed(&mut app, "dotfiles");
    press(&mut app, Key::Esc);
    assert!(!app.finder_open);
    assert_eq!(app.repo, repo);
}

#[test]
fn in_the_commit_picker_j_k_enter_and_esc() {
    let mut app = in_the_diff_body();
    ch(&mut app, 'v');
    ch(&mut app, 'j');
    ch(&mut app, 'j');
    ch(&mut app, 'k');
    assert_eq!(app.commit_sel, 1);
    press(&mut app, Key::Esc);
    assert!(!app.commits_open);
    assert!(app.diff_commit.is_none(), "esc chooses nothing");
    ch(&mut app, 'v');
    ch(&mut app, 'j');
    ch(&mut app, 'j');
    press(&mut app, Key::Enter);
    assert_eq!(app.diff_commit.as_deref(), Some("9b3e77d1c2a0"));
}

#[test]
fn in_the_review_dialog_tab_changes_the_verdict() {
    let mut app = in_the_diff_body();
    ch(&mut app, 'R');
    assert_eq!(app.review_verdict, Verdict::Comment);
    press(&mut app, Key::Tab);
    assert_eq!(app.review_verdict, Verdict::Approve);
    press(&mut app, Key::Tab);
    assert_eq!(app.review_verdict, Verdict::RequestChanges);
    press(&mut app, Key::Tab);
    assert_eq!(app.review_verdict, Verdict::Comment);
}

#[test]
fn in_the_help_j_and_k_scroll_and_esc_closes() {
    let mut app = seeded();
    ch(&mut app, '?');
    ch(&mut app, 'j');
    ch(&mut app, 'j');
    ch(&mut app, 'k');
    assert_eq!(app.help_scroll, 1);
    press(&mut app, Key::Esc);
    assert!(!app.help_open);
}

#[test]
fn in_the_account_switcher_k_moves_back_up() {
    let mut app = seeded();
    ch(&mut app, 'a');
    ch(&mut app, 'j');
    ch(&mut app, 'k');
    assert_eq!(app.acc_sel, 0);
}

#[test]
fn in_the_theme_picker_k_previews_the_previous_theme() {
    let mut app = seeded();
    let before = crate::tui::theme::current();
    ch(&mut app, 't');
    ch(&mut app, 'j');
    ch(&mut app, 'k');
    assert_eq!(crate::tui::theme::current(), before);
    press(&mut app, Key::Esc);
}

#[test]
fn enter_confirms_a_close_as_y_does() {
    let (mut app, tape) = recording();
    ch(&mut app, 'c');
    press(&mut app, Key::Enter);
    assert!(
        tape.0
            .borrow()
            .iter()
            .any(|r| matches!(r, Request::Close { .. }))
    );
}

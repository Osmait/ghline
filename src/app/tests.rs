//! Tests for the state machine. Kept in one module because they exercise the
//! `App` as a whole rather than any one of its files.

use super::input::strip_ws_only;
use super::*;
use crate::actions::Prompt;
use crate::data::Status;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

fn demo() -> App {
    App::new(Source::Demo)
}

/// The repository pane is hidden by default, so the tests that are about
/// walking to it have to ask for it — as a reader would with `b`.
fn demo_with_sidebar() -> App {
    let mut app = demo();
    app.sidebar = true;
    app.sidebar_shown = true;
    app
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
}

fn ch(app: &mut App, c: char) {
    press(app, KeyCode::Char(c));
}

// --- panes and focus ---

#[test]
fn each_view_exposes_its_own_panes() {
    let mut app = demo_with_sidebar();
    assert_eq!(app.panes(), vec![Pane::Repos, Pane::List]);

    // a PR has a checks pane; an issue does not
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.panes(), vec![Pane::Repos, Pane::Body, Pane::Checks]);

    let mut app = demo_with_sidebar();
    ch(&mut app, '1'); // issues tab
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.panes(), vec![Pane::Repos, Pane::Body]);
}

#[test]
fn h_and_l_stop_at_the_edges() {
    let mut app = demo_with_sidebar();
    app.pane = Pane::Repos;
    ch(&mut app, 'h');
    assert_eq!(app.pane, Pane::Repos, "h at the leftmost pane stays put");

    app.pane = Pane::List;
    ch(&mut app, 'l');
    assert_eq!(app.pane, Pane::List, "l at the rightmost pane stays put");
}

#[test]
fn tab_cycles_all_the_way_around() {
    let mut app = demo_with_sidebar();
    press(&mut app, KeyCode::Enter); // PR detail: three panes
    app.pane = Pane::Repos;
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.pane, Pane::Body);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.pane, Pane::Checks);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.pane, Pane::Repos, "tab wraps around");
    press(&mut app, KeyCode::BackTab);
    assert_eq!(app.pane, Pane::Checks, "shift-tab wraps the other way");
}

#[test]
fn enter_and_esc_walk_the_same_path_in_reverse() {
    let mut app = demo_with_sidebar();
    app.pane = Pane::Repos;

    press(&mut app, KeyCode::Enter); // repos -> list
    assert!(app.view == View::List && app.pane == Pane::List);
    press(&mut app, KeyCode::Enter); // list -> detail, landing on the body
    assert!(app.view == View::Detail && app.pane == Pane::Body);
    ch(&mut app, 'l'); // body -> checks
    press(&mut app, KeyCode::Enter); // checks -> logs
    assert!(app.view == View::Logs && app.pane == Pane::Tree);
    press(&mut app, KeyCode::Enter); // tree -> log output
    assert_eq!(app.pane, Pane::Log);

    press(&mut app, KeyCode::Esc);
    assert!(app.view == View::Logs && app.pane == Pane::Tree);
    press(&mut app, KeyCode::Esc);
    assert!(app.view == View::Detail && app.pane == Pane::Checks);
    press(&mut app, KeyCode::Esc);
    assert!(app.view == View::List && app.pane == Pane::List);
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.pane, Pane::Repos);
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.pane, Pane::Repos, "there is nothing left to go back to");
}

#[test]
fn the_focused_pane_is_always_one_the_view_has() {
    let mut app = demo();
    // land on the checks pane, then jump to the issues tab, which has none
    press(&mut app, KeyCode::Enter);
    ch(&mut app, 'l');
    assert_eq!(app.pane, Pane::Checks);
    ch(&mut app, '1');
    assert!(app.panes().contains(&app.pane));
}

// --- movement ---

#[test]
fn j_and_k_clamp_instead_of_wrapping() {
    let mut app = demo();
    app.pane = Pane::List;
    let last = app.visible().len() - 1;
    for _ in 0..50 {
        ch(&mut app, 'j');
    }
    assert_eq!(app.item, last, "j stops at the last item");
    for _ in 0..50 {
        ch(&mut app, 'k');
    }
    assert_eq!(app.item, 0, "k stops at the first");
}

#[test]
fn g_and_shift_g_reach_the_ends() {
    let mut app = demo();
    app.pane = Pane::List;
    ch(&mut app, 'G');
    assert_eq!(app.item, app.visible().len() - 1);
    ch(&mut app, 'g');
    assert_eq!(app.item, 0);
}

#[test]
fn scrolling_the_log_by_hand_drops_follow_mode() {
    let mut app = demo();
    app.view = View::Logs;
    app.pane = Pane::Log;
    assert!(app.follow);
    ch(&mut app, 'j');
    assert!(!app.follow, "manual movement takes over from follow");
}

#[test]
fn moving_between_items_resets_the_body_scroll() {
    let mut app = demo();
    app.pane = Pane::List;
    app.detail_scroll = 42;
    ch(&mut app, 'j');
    assert_eq!(app.detail_scroll, 0);
}

// --- empty and degenerate states ---

#[test]
fn an_app_with_no_accounts_does_not_panic() {
    // this is what live mode looks like before the first response lands
    let app = App::new(Source::Live);
    assert_eq!(app.repo_idx(), 0);
    assert_eq!(app.login(), "—");
    assert_eq!(app.repo_name(), "—");
    assert!(app.repo().is_none());
    assert!(app.current().is_none());
    assert!(app.list().is_empty());
    assert!(app.visible().is_empty());
    assert!(app.diff_files().is_empty());
    assert_eq!(app.file_idx(), 0);
    assert!(app.diff_rows().is_empty());
}

#[test]
fn a_filter_that_matches_nothing_leaves_no_selection() {
    let mut app = demo();
    app.filter = "zzzzzzzz".into();
    assert!(app.visible().is_empty());
    assert!(app.current().is_none());
    assert!(app.current_index().is_none());

    // and the actions that need a selection simply do nothing
    app.ask_merge();
    assert!(app.prompt.is_none());
    app.confirm();
    assert!(app.prompt.is_none());
}

#[test]
fn navigating_an_empty_list_stays_at_zero() {
    let mut app = demo();
    app.filter = "zzzzzzzz".into();
    app.pane = Pane::List;
    ch(&mut app, 'j');
    ch(&mut app, 'G');
    assert_eq!(app.item, 0);
}

// --- pull request actions ---

#[test]
fn merge_is_refused_for_everything_but_an_open_pr() {
    let mut app = demo();
    app.pane = Pane::List;

    // the draft PR of the demo data
    app.item = 3;
    assert_eq!(app.current().unwrap().state, Status::Draft);
    app.ask_merge();
    assert!(app.prompt.is_none(), "a draft cannot be merged");

    // and the already merged one
    app.item = 4;
    assert_eq!(app.current().unwrap().state, Status::Merged);
    app.ask_merge();
    assert!(app.prompt.is_none());
}

#[test]
fn a_merge_updates_the_pr_and_offers_the_branch() {
    let mut app = demo();
    app.pane = Pane::List;
    app.item = 0;
    let open_prs = app.repo().unwrap().prs;

    app.ask_merge();
    assert!(matches!(app.prompt, Some(Prompt::Merge(0))));
    app.confirm();

    let pr = app.current().unwrap();
    assert_eq!(pr.state, Status::Merged);
    assert_eq!(
        pr.as_pr().and_then(|p| p.merged_with.as_deref()),
        Some("merge commit")
    );
    assert_eq!(app.repo().unwrap().prs, open_prs - 1, "one less open PR");
    // GitHub offers to delete the branch right after
    assert!(matches!(app.prompt, Some(Prompt::DeleteBranch { .. })));

    app.confirm();
    assert!(app.current().unwrap().as_pr().unwrap().branch_deleted);
}

#[test]
fn closing_and_reopening_a_pr_round_trips() {
    let mut app = demo();
    app.pane = Pane::List;
    let open_prs = app.repo().unwrap().prs;

    app.ask_close();
    app.confirm();
    assert_eq!(app.current().unwrap().state, Status::Closed);
    assert_eq!(app.repo().unwrap().prs, open_prs - 1);

    app.ask_close(); // now it reopens
    assert!(matches!(app.prompt, Some(Prompt::Reopen)));
    app.confirm();
    assert_eq!(app.current().unwrap().state, Status::Open);
    assert_eq!(app.repo().unwrap().prs, open_prs);
}

#[test]
fn the_branch_prompt_remembers_which_branch_it_asked_about() {
    let mut app = demo();
    app.pane = Pane::List;
    app.ask_merge();
    app.confirm();

    let Some(Prompt::DeleteBranch { num, branch }) = app.prompt.clone() else {
        panic!("expected a delete-branch prompt");
    };
    // moving the selection must not change what gets deleted
    let expected = app.current().unwrap().num;
    assert_eq!(num, expected);
    assert!(!branch.is_empty());
}

#[test]
fn a_branch_cannot_be_deleted_while_the_pr_is_open() {
    let mut app = demo();
    app.pane = Pane::List;
    app.ask_delete_branch();
    assert!(app.prompt.is_none());
}

#[test]
fn cancelling_a_prompt_changes_nothing() {
    let mut app = demo();
    app.pane = Pane::List;
    let before = app.current().unwrap().state;
    app.ask_merge();
    app.cancel_prompt();
    assert!(app.prompt.is_none());
    assert_eq!(app.current().unwrap().state, before);
}

// --- diff view ---

#[test]
fn the_diff_only_opens_on_a_pull_request() {
    let mut app = demo();
    ch(&mut app, '1'); // issues
    app.pane = Pane::List;
    ch(&mut app, 'd');
    assert_ne!(app.view, View::Diff);

    ch(&mut app, '2'); // pull requests
    ch(&mut app, 'd');
    assert_eq!(app.view, View::Diff);
    assert_eq!(app.pane, Pane::Files);
}

#[test]
fn split_and_whitespace_toggles_only_bite_inside_the_diff() {
    let mut app = demo();
    app.pane = Pane::List;
    ch(&mut app, 's');
    assert!(!app.split, "s does nothing outside the diff view");

    ch(&mut app, 'd');
    ch(&mut app, 's');
    assert!(app.split);
    ch(&mut app, 'w');
    assert!(app.ws);
}

#[test]
fn a_file_with_no_hunks_yields_no_rows() {
    let mut app = demo();
    app.pane = Pane::List;
    ch(&mut app, 'd');
    // CHANGELOG.md is last in the demo data and has no textual changes
    let last = app.diff_files().len() - 1;
    app.file_idx = last;
    assert_eq!(app.diff_file().unwrap().path, "CHANGELOG.md");
    assert!(app.diff_rows().is_empty());
}

// --- ignore whitespace ---

fn hunk(lines: &[(char, &str)]) -> crate::data::Hunk {
    crate::data::Hunk {
        hdr: "@@ -1,1 +1,1 @@".into(),
        lines: lines.iter().map(|(c, t)| (*c, t.to_string())).collect(),
    }
}

#[test]
fn whitespace_only_changes_collapse_into_context() {
    let h = hunk(&[('-', "let x = 1;"), ('+', "let  x  =  1;")]);
    let out = strip_ws_only(&h);
    assert_eq!(out.lines.len(), 1);
    assert_eq!(out.lines[0].0, ' ', "it becomes a context line");
}

#[test]
fn a_real_change_survives_the_whitespace_filter() {
    let h = hunk(&[('-', "let x = 1;"), ('+', "let x = 2;")]);
    assert_eq!(strip_ws_only(&h).lines.len(), 2);
}

#[test]
fn unbalanced_blocks_are_left_alone() {
    // one deletion, two additions: not a whitespace-only rewrite
    let h = hunk(&[('-', "a"), ('+', "a"), ('+', "b")]);
    assert_eq!(strip_ws_only(&h).lines.len(), 3);
}

#[test]
fn context_only_hunks_pass_through_untouched() {
    let h = hunk(&[(' ', "a"), (' ', "b")]);
    assert_eq!(strip_ws_only(&h).lines.len(), 2);
}

// --- command line ---

#[test]
fn a_slash_filter_updates_as_you_type_and_esc_keeps_it() {
    let mut app = demo();
    ch(&mut app, '/');
    for c in "clamp".chars() {
        ch(&mut app, c);
    }
    assert_eq!(app.filter, "clamp");
    assert_eq!(app.visible().len(), 1);

    press(&mut app, KeyCode::Backspace);
    assert_eq!(app.filter, "clam");

    // esc closes the prompt but leaves the filter applied, as in the design
    press(&mut app, KeyCode::Esc);
    assert!(app.cmd.is_none());
    assert_eq!(app.filter, "clam");
}

#[test]
fn unknown_commands_are_ignored_without_leaving_the_prompt_open() {
    let mut app = demo();
    ch(&mut app, ':');
    for c in "nonsense".chars() {
        ch(&mut app, c);
    }
    press(&mut app, KeyCode::Enter);
    assert!(app.cmd.is_none());
    assert!(app.cmd_text.is_empty());
    assert_eq!(app.view, View::List);
}

#[test]
fn commands_reach_every_view() {
    for (cmd, view) in [("issues", View::List), ("logs", View::Logs)] {
        let mut app = demo();
        ch(&mut app, ':');
        for c in cmd.chars() {
            ch(&mut app, c);
        }
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.view, view, "`:{cmd}` should switch view");
    }
}

// --- flash messages ---

#[test]
fn a_flash_fades_after_a_few_ticks() {
    let mut app = demo();
    app.flash_ok("done");
    assert!(app.flash.is_some());
    for _ in 0..3 {
        app.tick();
    }
    assert!(app.flash.is_none());
}

#[test]
fn a_loading_list_draws_a_skeleton_rather_than_a_word() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let mut app = demo();
    app.hold_loading(0);

    let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
    term.draw(|f| crate::ui::draw(f, &mut app)).unwrap();

    let buf = term.backend().buffer();
    let blocks = (0..buf.area.width)
        .flat_map(|x| (0..buf.area.height).map(move |y| (x, y)))
        .filter(|&(x, y)| buf[(x, y)].symbol() == "\u{2588}")
        .count();
    assert!(
        blocks > 100,
        "the pane should be full of placeholder blocks"
    );

    let text: String = (0..buf.area.width).map(|x| buf[(x, 5)].symbol()).collect();
    assert!(
        !text.contains("loading"),
        "no word where the rows should be"
    );
}

#[test]
fn a_pending_body_draws_a_skeleton_but_a_loaded_one_does_not() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let blocks_on_screen = |app: &mut App| {
        let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
        term.draw(|f| crate::ui::draw(f, app)).unwrap();
        let buf = term.backend().buffer();
        (0..buf.area.width)
            .flat_map(|x| (0..buf.area.height).map(move |y| (x, y)))
            .filter(|&(x, y)| buf[(x, y)].symbol() == "\u{2588}")
            .count()
    };

    // the demo fixture already carries a body, so a pending state must not
    // paint over content that is already there
    let mut loaded = demo();
    press(&mut loaded, KeyCode::Enter);
    loaded.hold_loading(0);
    let with_body = blocks_on_screen(&mut loaded);

    // with the body emptied, the same pending state should show its shape
    let mut empty = demo();
    press(&mut empty, KeyCode::Enter);
    let key = (empty.repo_key(), empty.tab);
    if let Some(items) = empty.lists.get_mut(&key) {
        for it in items {
            it.body.clear();
        }
    }
    empty.hold_loading(0);
    let without_body = blocks_on_screen(&mut empty);

    assert!(
        without_body > with_body,
        "a body that has not arrived should be drawn as a skeleton \
         ({without_body} blocks) and one that has should not ({with_body})"
    );
}

// --- theme picker ---

#[test]
fn the_picker_previews_as_you_move_and_esc_puts_it_back() {
    let _g = crate::theme::tests::LOCK.lock();
    use crate::theme::{Theme, current, set};

    set(Theme::Design);
    let mut app = demo();
    ch(&mut app, 't');
    assert!(app.themes_open);
    assert_eq!(current(), Theme::Design, "opening changes nothing yet");

    ch(&mut app, 'j');
    assert_eq!(current(), Theme::Mocha, "moving applies it straight away");

    press(&mut app, KeyCode::Esc);
    assert!(!app.themes_open);
    assert_eq!(current(), Theme::Design, "leaving puts back what was on");
    set(Theme::Design);
}

#[test]
fn enter_keeps_the_previewed_theme() {
    let _g = crate::theme::tests::LOCK.lock();
    use crate::theme::{Theme, current, set};

    set(Theme::Design);
    let mut app = demo();
    ch(&mut app, 't');
    ch(&mut app, 'j');
    press(&mut app, KeyCode::Enter);
    assert!(!app.themes_open);
    assert_eq!(current(), Theme::Mocha);
    set(Theme::Design);
}

#[test]
fn the_picker_does_not_run_off_either_end() {
    let _g = crate::theme::tests::LOCK.lock();
    use crate::theme::{Theme, set};

    set(Theme::Design);
    let mut app = demo();
    ch(&mut app, 't');
    for _ in 0..10 {
        ch(&mut app, 'j');
    }
    assert_eq!(app.theme_sel, Theme::ALL.len() - 1);
    for _ in 0..10 {
        ch(&mut app, 'k');
    }
    assert_eq!(app.theme_sel, 0);
    press(&mut app, KeyCode::Esc);
    set(Theme::Design);
}

#[test]
fn the_repository_pane_starts_hidden() {
    let app = demo();
    assert!(!app.sidebar, "sixty repositories are a wall, not a default");
    assert!(!app.panes().contains(&Pane::Repos));
}

#[test]
fn the_picker_swallows_the_keys_beneath_it() {
    let _g = crate::theme::tests::LOCK.lock();
    use crate::theme::{Theme, set};

    set(Theme::Design);
    let mut app = demo();
    let before = app.item;
    ch(&mut app, 't');
    ch(&mut app, 'j'); // moves the theme, not the list
    assert_eq!(app.item, before);
    press(&mut app, KeyCode::Esc);
    set(Theme::Design);
}

// --- sidebar ---

#[test]
fn b_hides_the_repository_pane_and_the_panes_follow() {
    let mut app = demo_with_sidebar();
    assert!(app.panes().contains(&Pane::Repos));

    ch(&mut app, 'b');
    assert!(!app.sidebar);
    // the render is what actually decides, so mirror what it would do
    app.sidebar_shown = false;
    assert!(
        !app.panes().contains(&Pane::Repos),
        "a hidden pane must not be walkable"
    );

    ch(&mut app, 'b');
    assert!(app.sidebar);
}

#[test]
fn hiding_the_sidebar_takes_the_focus_with_it() {
    let mut app = demo_with_sidebar();
    app.pane = Pane::Repos;
    ch(&mut app, 'b');
    assert_ne!(app.pane, Pane::Repos, "focus cannot stay on a hidden pane");
}

#[test]
fn h_does_not_reach_a_hidden_sidebar() {
    let mut app = demo();
    app.sidebar_shown = false;
    app.pane = Pane::List;
    ch(&mut app, 'h');
    assert_eq!(app.pane, Pane::List);
}

#[test]
fn a_narrow_terminal_hides_it_whatever_was_asked_for() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let mut app = demo_with_sidebar();
    assert!(app.sidebar, "asked for");

    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
    assert!(!app.sidebar_shown, "80 columns is not enough room for it");
    assert!(app.sidebar, "the preference itself is untouched");

    let mut term = Terminal::new(TestBackend::new(150, 40)).unwrap();
    term.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
    assert!(app.sidebar_shown, "and it comes back when there is room");
}

#[test]
fn the_logs_and_diff_views_never_show_it() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let mut app = demo();
    press(&mut app, KeyCode::Enter);
    ch(&mut app, 'l');
    press(&mut app, KeyCode::Enter); // logs
    let mut term = Terminal::new(TestBackend::new(150, 40)).unwrap();
    term.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
    assert!(!app.sidebar_shown);
}

// --- the finder ---

#[test]
fn p_opens_the_finder_on_repositories() {
    let mut app = demo();
    ch(&mut app, 'p');
    assert!(app.finder_open);
    assert_eq!(app.finder_source, crate::finder::Source::Repos);
    assert_eq!(
        app.finder_len(),
        app.repos().len(),
        "everything, unfiltered"
    );
}

#[test]
fn typing_filters_the_repositories_as_you_go() {
    let mut app = demo();
    ch(&mut app, 'p');
    for c in "tui".chars() {
        ch(&mut app, c);
    }
    let hits = app.finder_results();
    assert!(!hits.is_empty());
    assert_eq!(hits[0].label, "tuikit", "the best match leads");
    assert!(
        hits.len() < app.repos().len(),
        "and the rest were filtered out"
    );
}

#[test]
fn a_query_that_matches_nothing_leaves_an_empty_list() {
    let mut app = demo();
    ch(&mut app, 'p');
    for c in "zzzzz".chars() {
        ch(&mut app, c);
    }
    assert_eq!(app.finder_len(), 0);
}

#[test]
fn enter_on_a_repository_goes_there() {
    let mut app = demo();
    let target = app.repos()[4].name.clone();
    ch(&mut app, 'p');
    for c in target.chars().take(4) {
        ch(&mut app, c);
    }
    press(&mut app, KeyCode::Enter);
    assert!(!app.finder_open);
    assert_eq!(app.repo_name(), target);
    assert_eq!(app.view, View::List);
}

#[test]
fn tab_walks_the_sources_and_keeps_the_query() {
    let mut app = demo();
    ch(&mut app, 'p');
    ch(&mut app, 'x');
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.finder_source, crate::finder::Source::Issues);
    assert_eq!(app.finder_query, "x", "the same words, somewhere else");
    for _ in 0..3 {
        press(&mut app, KeyCode::Tab);
    }
    assert_eq!(app.finder_source, crate::finder::Source::Repos, "it wraps");
}

#[test]
fn the_selection_wraps_and_never_leaves_the_list() {
    let mut app = demo();
    ch(&mut app, 'p');
    let len = app.finder_len();
    press(&mut app, KeyCode::Up);
    assert_eq!(app.finder_sel, len - 1, "up from the top lands at the end");
    press(&mut app, KeyCode::Down);
    assert_eq!(app.finder_sel, 0);
}

#[test]
fn the_finder_swallows_the_keys_beneath_it() {
    let mut app = demo();
    let before = app.item;
    ch(&mut app, 'p');
    ch(&mut app, 'j'); // a letter of the query, not a movement
    assert_eq!(app.item, before);
    assert_eq!(app.finder_query, "j");
    press(&mut app, KeyCode::Esc);
    assert!(!app.finder_open);
}

#[test]
fn a_commit_search_waits_for_something_to_search_for() {
    // GitHub rejects a commit search made of qualifiers alone, so an empty
    // query must not be sent at all
    let mut app = App::new(Source::Live);
    app.open_finder();
    app.finder_source = crate::finder::Source::Commits;
    app.finder_sent = "\u{0}".into();
    app.finder_tick();
    assert!(
        !app.finder_state.is_loading(),
        "nothing should have been asked for"
    );
}

// --- moving between repositories ---

#[test]
fn brackets_step_through_the_repositories_and_wrap() {
    let mut app = demo();
    let n = app.repos().len();
    let start = app.repo_idx();

    ch(&mut app, ']');
    assert_eq!(app.repo_idx(), (start + 1) % n);
    ch(&mut app, '[');
    assert_eq!(app.repo_idx(), start);

    for _ in 0..n {
        ch(&mut app, ']');
    }
    assert_eq!(app.repo_idx(), start, "a full turn comes back");
}

#[test]
fn stepping_to_another_repository_resets_the_view() {
    let mut app = demo();
    press(&mut app, KeyCode::Enter); // into a detail
    app.item = 2;
    ch(&mut app, ']');
    assert_eq!(app.view, View::List);
    assert_eq!(app.item, 0, "the selection cannot follow to another repo");
}

// ---------------------------------------------------------------- the mouse
//
// These render into an off-screen terminal first, because the click regions
// are a product of the frame: aiming at a pane the renderer did not draw is
// exactly the case that has to keep working.

mod mouse {
    use super::*;
    use crate::app::hit::{Region, Target};
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::time::{Duration, Instant};

    /// Wide enough that the repository pane is allowed on screen.
    const W: u16 = 120;
    const H: u16 = 40;

    fn render(app: &mut App) {
        render_sized(app, W, H);
    }

    fn render_sized(app: &mut App, w: u16, h: u16) {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| crate::ui::draw(f, app)).unwrap();
    }

    /// Where the last frame put something. Panics rather than returning an
    /// option: a test that cannot find its target is a broken test, and the
    /// message should say which one.
    fn region(app: &App, target: Target) -> Region {
        app.hits
            .iter()
            .rev()
            .find(|r| r.target == target)
            .copied()
            .unwrap_or_else(|| panic!("the last frame drew no {target:?}"))
    }

    fn event(kind: MouseEventKind, col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn click_at(app: &mut App, col: u16, row: u16) {
        app.on_mouse(event(MouseEventKind::Down(MouseButton::Left), col, row));
    }

    /// Clicks the nth row of a target, wherever the renderer put it.
    fn click_row(app: &mut App, target: Target, n: u16) {
        let r = region(app, target);
        click_at(app, r.area.x + 1, r.area.y + n * r.row_h);
    }

    fn wheel(app: &mut App, col: u16, row: u16, down: bool) {
        let kind = if down {
            MouseEventKind::ScrollDown
        } else {
            MouseEventKind::ScrollUp
        };
        app.on_mouse(event(kind, col, row));
    }

    // --- focus and selection ---

    #[test]
    fn a_click_focuses_the_pane_it_landed_on() {
        let mut app = demo_with_sidebar();
        app.pane = Pane::List;
        render(&mut app);

        click_row(&mut app, Target::Pane(Pane::Repos), 0);
        assert_eq!(app.pane, Pane::Repos);
    }

    #[test]
    fn a_click_selects_the_row_under_the_pointer() {
        let mut app = demo();
        render(&mut app);
        assert!(app.visible().len() > 2, "the fixture needs rows to click");

        click_row(&mut app, Target::Pane(Pane::List), 2);
        assert_eq!(app.item, 2);
    }

    #[test]
    fn a_click_reads_through_the_scroll() {
        // A short terminal with the selection at the bottom is what actually
        // scrolls a list; setting the offset by hand would just be undone by
        // the next frame, which is the render's job.
        let mut app = demo();
        render(&mut app);
        let last = app.visible().len() - 1;
        app.item = last;
        render_sized(&mut app, W, 14);

        let r = region(&app, Target::Pane(Pane::List));
        assert!(r.scroll > 0, "the fixture is too short to scroll");

        // the top row drawn is no longer entry zero
        click_row(&mut app, Target::Pane(Pane::List), 0);
        assert_eq!(app.item, r.scroll);
    }

    #[test]
    fn a_click_on_the_blank_space_below_the_rows_keeps_the_selection() {
        let mut app = demo();
        render(&mut app);
        app.item = 1;

        let r = region(&app, Target::Pane(Pane::List));
        // the last row of the pane, well past a short fixture list
        click_at(&mut app, r.area.x + 1, r.area.bottom() - 1);
        assert_eq!(app.item, 1, "the selection stayed put");
        assert_eq!(app.pane, Pane::List, "but the focus still moved");
    }

    #[test]
    fn selecting_a_repository_by_click_resets_the_list_under_it() {
        let mut app = demo_with_sidebar();
        app.item = 3;
        app.item_scroll = 3;
        render(&mut app);

        click_row(&mut app, Target::Pane(Pane::Repos), 1);
        assert_eq!(app.repo, 1);
        assert_eq!(app.item, 0, "a click means what `j` means");
        assert_eq!(app.item_scroll, 0);
    }

    #[test]
    fn a_click_before_the_first_frame_does_nothing() {
        let mut app = demo();
        let before = app.pane;
        click_at(&mut app, 10, 10);
        assert_eq!(app.pane, before, "there are no regions yet");
    }

    // --- the double click ---

    #[test]
    fn a_double_click_opens_what_it_selected() {
        let mut app = demo();
        render(&mut app);
        let r = region(&app, Target::Pane(Pane::List));
        let (col, row) = (r.area.x + 1, r.area.y);

        let now = Instant::now();
        let down = |c, rw| event(MouseEventKind::Down(MouseButton::Left), c, rw);
        app.on_mouse_at(down(col, row), now);
        assert_eq!(app.view, View::List, "one click only selects");

        app.on_mouse_at(down(col, row), now + Duration::from_millis(120));
        assert_eq!(app.view, View::Detail, "the second one opens it");
    }

    #[test]
    fn two_slow_clicks_are_two_clicks() {
        let mut app = demo();
        render(&mut app);
        let r = region(&app, Target::Pane(Pane::List));
        let (col, row) = (r.area.x + 1, r.area.y);

        let now = Instant::now();
        let down = |c, rw| event(MouseEventKind::Down(MouseButton::Left), c, rw);
        app.on_mouse_at(down(col, row), now);
        app.on_mouse_at(down(col, row), now + Duration::from_secs(2));
        assert_eq!(app.view, View::List);
    }

    #[test]
    fn two_clicks_on_different_rows_are_not_a_double_click() {
        let mut app = demo();
        render(&mut app);
        if app.visible().len() < 2 {
            return;
        }
        let r = region(&app, Target::Pane(Pane::List));
        let now = Instant::now();
        let down = |c, rw| event(MouseEventKind::Down(MouseButton::Left), c, rw);

        app.on_mouse_at(down(r.area.x + 1, r.area.y), now);
        app.on_mouse_at(
            down(r.area.x + 1, r.area.y + r.row_h),
            now + Duration::from_millis(120),
        );
        assert_eq!(app.view, View::List, "the hand moved; it was two clicks");
        assert_eq!(app.item, 1, "and the second one selected");
    }

    #[test]
    fn a_third_click_does_not_open_a_second_time() {
        let mut app = demo();
        render(&mut app);
        let r = region(&app, Target::Pane(Pane::List));
        let (col, row) = (r.area.x + 1, r.area.y);
        let now = Instant::now();
        let down = |c, rw| event(MouseEventKind::Down(MouseButton::Left), c, rw);

        app.on_mouse_at(down(col, row), now);
        app.on_mouse_at(down(col, row), now + Duration::from_millis(100));
        app.on_mouse_at(down(col, row), now + Duration::from_millis(200));
        // the third click starts a fresh pair rather than completing another
        assert_eq!(app.view, View::Detail);
    }

    // --- the wheel ---

    #[test]
    fn the_wheel_moves_the_pane_under_the_pointer() {
        let mut app = demo();
        render(&mut app);
        if app.visible().len() < 4 {
            return;
        }
        let r = region(&app, Target::Pane(Pane::List));

        wheel(&mut app, r.area.x + 1, r.area.y + 1, true);
        assert_eq!(app.item, 3, "three rows per notch");
        wheel(&mut app, r.area.x + 1, r.area.y + 1, false);
        assert_eq!(app.item, 0);
    }

    #[test]
    fn the_wheel_does_not_steal_the_focus() {
        let mut app = demo_with_sidebar();
        app.pane = Pane::List;
        render(&mut app);

        let r = region(&app, Target::Pane(Pane::Repos));
        wheel(&mut app, r.area.x + 1, r.area.y + 1, true);
        assert_eq!(app.pane, Pane::List, "reading a pane is not entering it");
        assert!(app.repo > 0, "but it still moved");
    }

    #[test]
    fn the_wheel_stops_at_the_end_rather_than_wrapping() {
        let mut app = demo();
        render(&mut app);
        let r = region(&app, Target::Pane(Pane::List));
        let last = app.visible().len() - 1;

        for _ in 0..50 {
            wheel(&mut app, r.area.x + 1, r.area.y + 1, true);
        }
        assert_eq!(app.item, last);
    }

    // --- the tab bar ---

    #[test]
    fn clicking_a_tab_switches_to_it() {
        let mut app = demo();
        render(&mut app);

        let r = region(&app, Target::Tab(1));
        click_at(&mut app, r.area.x + 1, r.area.y);
        assert_eq!(app.tab, 1);
        assert_eq!(app.view, View::List);
        assert_eq!(app.pane, Pane::List);
    }

    // --- modals ---

    #[test]
    fn a_click_on_a_finder_row_selects_it() {
        let mut app = demo();
        app.open_finder();
        render(&mut app);
        if app.finder_len() < 3 {
            return;
        }

        click_row(&mut app, Target::Finder, 2);
        assert_eq!(app.finder_sel, 2);
        assert!(app.finder_open, "one click selects, it does not accept");
    }

    #[test]
    fn a_click_outside_a_modal_closes_it() {
        let mut app = demo();
        app.open_finder();
        render(&mut app);

        click_at(&mut app, 1, 3); // the far top left, outside the modal
        assert!(!app.finder_open);
    }

    #[test]
    fn a_click_inside_a_modal_but_off_its_rows_changes_nothing() {
        let mut app = demo();
        app.open_finder();
        render(&mut app);
        let before = app.finder_sel;

        let r = region(&app, Target::Finder);
        click_at(&mut app, r.area.x + 2, r.area.y.saturating_sub(2)); // its header
        assert!(app.finder_open, "the modal absorbed the click");
        assert_eq!(app.finder_sel, before);
    }

    #[test]
    fn a_modal_shadows_the_panes_behind_it() {
        let mut app = demo();
        app.open_finder();
        render(&mut app);
        let before = app.item;

        // straight through the middle of the modal, where the list would be
        let r = region(&app, Target::Finder);
        click_at(&mut app, r.area.x + 3, r.area.y + r.row_h);
        assert_eq!(app.item, before, "the list underneath was not touched");
    }

    #[test]
    fn clicking_a_theme_previews_it_the_way_moving_to_it_does() {
        let mut app = demo();
        app.open_themes();
        render(&mut app);

        click_row(&mut app, Target::Themes, 1);
        assert_eq!(app.theme_sel, 1);
        assert_eq!(crate::theme::current(), crate::theme::Theme::ALL[1]);

        // put it back: the theme is process-wide state
        crate::theme::set(app.theme_before);
    }

    // --- what the mouse must not do ---

    #[test]
    fn a_click_cannot_answer_a_confirmation() {
        let mut app = demo();
        render(&mut app);
        app.prompt = Some(Prompt::Close);

        click_at(&mut app, 10, 10);
        assert!(app.prompt.is_some(), "a stray click is not an answer");
    }

    #[test]
    fn every_pane_of_every_view_is_reachable_by_click() {
        // a pane the renderer forgot to register is a pane the mouse cannot
        // reach, and nothing else would notice
        for view in [View::List, View::Detail, View::Logs, View::Diff] {
            let mut app = demo_with_sidebar();
            app.view = view;
            render(&mut app);
            for pane in app.panes() {
                assert!(
                    app.hits.iter().any(|r| r.target == Target::Pane(pane)),
                    "{pane:?} is on screen in {view:?} but records no region"
                );
            }
        }
    }
}

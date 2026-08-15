//! Tests for the state machine. Kept in one module because they exercise the
//! `App` as a whole rather than any one of its files.

use super::input::strip_ws_only;
use super::*;
use crate::github::actions::Prompt;
use crate::github::data::Status;
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

fn hunk(lines: &[(char, &str)]) -> crate::github::data::Hunk {
    crate::github::data::Hunk {
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
    term.draw(|f| crate::github::ui::draw(f, &mut app)).unwrap();

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
        term.draw(|f| crate::github::ui::draw(f, app)).unwrap();
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
    let _g = crate::shared::theme::tests::LOCK.lock();
    use crate::shared::theme::{Theme, current, set};

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
    let _g = crate::shared::theme::tests::LOCK.lock();
    use crate::shared::theme::{Theme, current, set};

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
    let _g = crate::shared::theme::tests::LOCK.lock();
    use crate::shared::theme::{Theme, set};

    set(Theme::Design);
    let mut app = demo();
    ch(&mut app, 't');
    for _ in 0..10 {
        ch(&mut app, 'j');
    }
    assert_eq!(app.theme_sel, Theme::all().len() - 1);
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
    let _g = crate::shared::theme::tests::LOCK.lock();
    use crate::shared::theme::{Theme, set};

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
    term.draw(|f| crate::github::ui::draw(f, &mut app)).unwrap();
    assert!(!app.sidebar_shown, "80 columns is not enough room for it");
    assert!(app.sidebar, "the preference itself is untouched");

    let mut term = Terminal::new(TestBackend::new(150, 40)).unwrap();
    term.draw(|f| crate::github::ui::draw(f, &mut app)).unwrap();
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
    term.draw(|f| crate::github::ui::draw(f, &mut app)).unwrap();
    assert!(!app.sidebar_shown);
}

// --- the finder ---

#[test]
fn p_opens_the_finder_on_repositories() {
    let mut app = demo();
    ch(&mut app, 'p');
    assert!(app.finder_open);
    assert_eq!(app.finder_source, crate::github::finder::Source::Repos);
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
    assert_eq!(app.finder_source, crate::github::finder::Source::Issues);
    assert_eq!(app.finder_query, "x", "the same words, somewhere else");
    for _ in 0..3 {
        press(&mut app, KeyCode::Tab);
    }
    assert_eq!(
        app.finder_source,
        crate::github::finder::Source::Repos,
        "it wraps"
    );
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
    app.finder_source = crate::github::finder::Source::Commits;
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
    use crate::github::app::hit::{Region, Target};
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
        term.draw(|f| crate::github::ui::draw(f, app)).unwrap();
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
        assert_eq!(
            crate::shared::theme::current(),
            crate::shared::theme::Theme::all()[1]
        );

        // put it back: the theme is process-wide state
        crate::shared::theme::set(app.theme_before);
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

// ----------------------------------------------------- all repositories
//
// The pseudo-repository puts items from many repositories into one list, so
// the selection and the pane it lives in no longer agree about where a thing
// is. Getting that wrong would merge a pull request in the wrong repository,
// which is why it is worth pinning down.

mod all_repos {
    use super::*;
    use crate::github::data::Repo;

    /// The demo with a gathering row in front, as live mode builds it.
    fn gathered() -> App {
        let mut app = demo();
        if let Some(a) = app.accounts.get_mut(app.acc) {
            a.repos.insert(0, Repo::all(&a.repos));
        }
        app.repo = 0;
        app
    }

    /// Files a list under the gathering row with every row stamped with the
    /// repository it came from, which is what the live search returns.
    fn stamp(app: &mut App, repo: &str) {
        let key = (app.repo_key(), app.tab);
        let mut items = if app.tab == 1 {
            demo::prs(0)
        } else {
            demo::issues(0)
        };
        for it in &mut items {
            it.repo = repo.to_string();
        }
        app.lists.insert(key, items);
    }

    #[test]
    fn the_gathering_row_is_named_not_keyed() {
        let all = Repo::all(&[]);
        assert_eq!(all.name, "*", "the key can never collide with a real name");
        assert_eq!(
            all.label(),
            "all repositories",
            "but it is not what is shown"
        );
        assert!(all.is_all());
    }

    #[test]
    fn it_adds_up_what_it_gathers() {
        let repos = vec![
            Repo {
                issues: 3,
                prs: 1,
                has_workflows: false,
                ..Repo::empty()
            },
            Repo {
                issues: 4,
                prs: 2,
                has_workflows: true,
                ..Repo::empty()
            },
        ];
        let all = Repo::all(&repos);
        assert_eq!(all.issues, 7);
        assert_eq!(all.prs, 3);
        assert!(all.has_workflows, "one repository with CI is enough");
    }

    #[test]
    fn an_item_with_no_repository_of_its_own_belongs_to_the_pane() {
        // every single-repository list, which is to say almost all of them
        let app = demo();
        assert_eq!(app.item_repo_key(), app.repo_key());
    }

    #[test]
    fn an_item_from_a_gathered_list_keeps_its_own_repository() {
        let mut app = gathered();
        assert!(app.is_all(), "the gathering row is selected");
        stamp(&mut app, "someone/elsewhere");

        assert_eq!(app.repo_key(), "marasanz/*", "the list is filed under all");
        assert_eq!(
            app.item_repo_key(),
            "someone/elsewhere",
            "but the item is not"
        );
    }

    #[test]
    fn merging_from_a_gathered_list_would_act_on_the_items_repository() {
        // the same question the merge, close and branch-delete requests ask
        let mut app = gathered();
        app.tab = 1;
        stamp(&mut app, "someone/elsewhere");
        assert_eq!(app.item_repo_key(), "someone/elsewhere");

        // and back on a real repository it is the pane's again
        app.repo = 1;
        assert!(!app.is_all());
        assert_eq!(app.item_repo_key(), app.repo_key());
    }

    #[test]
    fn the_filter_reaches_the_repository_a_row_came_from() {
        let mut app = gathered();
        let key = (app.repo_key(), app.tab);
        stamp(&mut app, "marasanz/haystack");
        let total = app.visible().len();
        assert!(total > 1, "the fixture needs rows");

        // a word that is in no title, only in a repository name
        if let Some(items) = app.lists.get_mut(&key) {
            items[0].repo = "marasanz/needle".into();
            for it in items.iter_mut().skip(1) {
                it.repo = "marasanz/haystack".into();
            }
        }
        app.filter = "needle".into();
        assert_eq!(app.visible().len(), 1, "matched on the repository alone");
    }

    #[test]
    fn only_repositories_with_workflows_are_asked_for_runs() {
        let mut app = gathered();
        if let Some(a) = app.accounts.get_mut(0) {
            a.repos[1].has_workflows = false;
        }
        let asked = app.workflow_repos();

        assert!(
            !asked.iter().any(|r| r.ends_with("/*")),
            "the gathering row is not a repository to call"
        );
        let skipped = app.repos()[1].name.clone();
        assert!(
            !asked.iter().any(|r| r.ends_with(&format!("/{skipped}"))),
            "a repository with no workflows is not worth a call"
        );
        assert!(!asked.is_empty(), "the rest still are");
    }

    #[test]
    fn a_session_starts_on_the_gathering_row() {
        // it is inserted first, and the selection starts at zero
        let app = gathered();
        assert_eq!(app.repo_idx(), 0);
        assert!(app.is_all());
        assert_eq!(app.repo_label(), "all repositories");
    }

    #[test]
    fn an_answer_lands_on_the_row_it_is_about_not_the_first_with_that_number() {
        // A gathered list genuinely holds two different #14s. Matching on the
        // number alone would write one pull request's body onto the other, and
        // nothing would say so.
        use crate::github::service::Response;

        let mut app = gathered();
        app.tab = 1;
        let key = (app.repo_key(), 1);

        let mut a = demo::prs(0).remove(0);
        a.num = 14;
        a.repo = format!("{}/sbql", app.login());
        a.body = String::new();
        let mut b = a.clone();
        b.repo = format!("{}/accounting", app.login());
        app.lists.insert(key.clone(), vec![a, b]);

        app.apply(Response::PrDetail {
            repo: format!("{}/accounting", app.login()),
            num: 14,
            result: Ok(("the accounting one".into(), Vec::new(), Vec::new())),
        });

        let items = &app.lists[&key];
        assert_eq!(items[0].body, "", "sbql#14 was not the one asked about");
        assert_eq!(items[1].body, "the accounting one");
    }

    #[test]
    fn an_answer_still_reaches_a_plain_single_repository_list() {
        use crate::github::service::Response;

        let mut app = demo();
        app.tab = 1;
        let key = (app.repo_key(), 1);
        let num = app.lists[&key][0].num;

        app.apply(Response::PrDetail {
            repo: app.repo_key(),
            num,
            result: Ok(("straight in".into(), Vec::new(), Vec::new())),
        });
        assert_eq!(app.lists[&key][0].body, "straight in");
    }
}

// -------------------------------------------------------------- dispatching
//
// Sending an issue makes a machine start working. The rules about where it
// may not go are the part worth pinning down.

mod dispatch {
    use super::*;
    use crate::github::app::Dest;
    use crate::shared::mux::{Agent, AgentStatus};

    fn agent(kind: &str, status: AgentStatus, focused: bool) -> Agent {
        Agent {
            kind: kind.into(),
            status,
            cwd: format!("/home/x/orca/{kind}-work"),
            pane: "wA:p1".into(),
            title: "doing something".into(),
            focused,
        }
    }

    fn with(agents: Vec<Agent>) -> App {
        let mut app = demo();
        app.agents = agents;
        app
    }

    #[test]
    fn an_idle_agent_will_take_it() {
        let d = Dest::Running(agent("claude", AgentStatus::Idle, false));
        assert_eq!(d.refusal(), None);
    }

    #[test]
    fn a_working_agent_will_not() {
        let d = Dest::Running(agent("claude", AgentStatus::Working, false));
        assert!(
            d.refusal().is_some_and(|w| w.contains("context")),
            "and it should say why"
        );
    }

    #[test]
    fn an_agent_stopped_on_a_question_will_not() {
        // the task would be read as the answer to the permission prompt
        let d = Dest::Running(agent("claude", AgentStatus::Blocked, false));
        assert!(d.refusal().is_some());
    }

    #[test]
    fn an_agent_in_an_unknown_state_will_not() {
        assert!(
            Dest::Running(agent("pi", AgentStatus::Unknown, false))
                .refusal()
                .is_some()
        );
    }

    #[test]
    fn the_window_showing_the_list_will_not_take_it() {
        // this program appears in its own agent list; sending to it is legal,
        // useless, and confusing
        let d = Dest::Running(agent("claude", AgentStatus::Idle, true));
        assert!(
            d.refusal().is_some_and(|w| w.contains("this window")),
            "being focused beats being idle"
        );
    }

    #[test]
    fn the_ones_that_can_take_it_are_offered_first() {
        let app = with(vec![
            agent("a", AgentStatus::Working, false),
            agent("b", AgentStatus::Idle, false),
            agent("c", AgentStatus::Working, false),
            agent("d", AgentStatus::Done, false),
        ]);
        let dests = app.dispatch_dests();
        let free: Vec<bool> = dests.iter().map(|d| d.refusal().is_none()).collect();
        assert_eq!(free, vec![true, true, false, false]);
    }

    #[test]
    fn a_refused_destination_is_listed_rather_than_hidden() {
        let app = with(vec![agent("a", AgentStatus::Working, false)]);
        assert_eq!(
            app.dispatch_dests().len(),
            1,
            "\"everything is busy\" beats an empty box"
        );
    }

    #[test]
    fn accepting_a_refused_destination_does_not_send_it() {
        let mut app = with(vec![agent("a", AgentStatus::Working, false)]);
        app.dispatch_open = true;
        app.dispatch_sel = 0;
        app.dispatch_accept();

        assert!(app.prompt.is_none(), "nothing was queued");
        assert!(app.dispatch_open, "and the picker stayed open to say so");
    }

    #[test]
    fn accepting_a_free_destination_asks_first() {
        let mut app = with(vec![agent("claude", AgentStatus::Idle, false)]);
        app.dispatch_open = true;
        app.dispatch_accept();

        assert!(!app.dispatch_open, "the picker closed");
        match app.prompt {
            Some(Prompt::Dispatch {
                ref pane, ref text, ..
            }) => {
                assert_eq!(
                    pane, "wA:p1",
                    "addressed by pane, which is what herdr takes"
                );
                assert!(text.contains('#'), "the rendered prompt names the issue");
            }
            _ => panic!("a confirmation should be pending, not a send"),
        }
    }

    // --- the three outcomes of "where would it even go" ---

    fn scanned(app: &mut App, slug: Option<&str>) {
        app.clones_state = Load::Ready;
        if let Some(slug) = slug {
            app.clones
                .insert(slug.into(), std::path::PathBuf::from("/home/x/orca/thing"));
        }
    }

    #[test]
    fn a_cloned_repository_offers_a_worktree_per_agent() {
        let mut app = with(vec![]);
        let repo = app.item_repo_key();
        scanned(&mut app, Some(&repo));

        let fresh: Vec<String> = app
            .dispatch_dests()
            .iter()
            .filter_map(|d| match d {
                Dest::Fresh { kind, .. } => Some(kind.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(fresh, crate::shared::config::agent_kinds());
    }

    #[test]
    fn a_repository_that_is_not_here_says_so_instead_of_offering_nothing() {
        let mut app = with(vec![]);
        scanned(&mut app, None);

        match app.dispatch_dests().as_slice() {
            [Dest::NotCloned(repo)] => {
                assert_eq!(*repo, app.item_repo_key());
            }
            other => panic!("expected one NotCloned, got {} rows", other.len()),
        }
    }

    #[test]
    fn nothing_is_offered_while_the_disk_is_still_being_walked() {
        // an empty index and an unfinished scan look the same; claiming the
        // repository is missing before looking would be a lie
        let mut app = with(vec![]);
        app.clones_state = Load::Loading;
        assert!(app.dispatch_dests().is_empty());
    }

    #[test]
    fn a_repository_that_is_not_here_cannot_be_dispatched_to() {
        let mut app = with(vec![]);
        scanned(&mut app, None);
        app.dispatch_open = true;
        app.dispatch_sel = 0;
        app.dispatch_accept();

        assert!(app.prompt.is_none());
        assert!(app.pending_fresh.is_none());
    }

    #[test]
    fn choosing_a_worktree_carries_the_plan_the_service_will_need() {
        let mut app = with(vec![]);
        let repo = app.item_repo_key();
        scanned(&mut app, Some(&repo));
        app.dispatch_open = true;
        // past the agents (there are none here) onto the first worktree
        app.dispatch_sel = 0;
        app.dispatch_accept();

        let plan = app.pending_fresh.expect("a worktree needs a plan");
        assert_eq!(plan.repo_root, "/home/x/orca/thing");
        assert!(
            plan.branch
                .as_deref()
                .is_some_and(|b| b.starts_with("issue-")),
            "the branch names the issue, so a second dispatch collides loudly"
        );
        assert!(app.prompt.is_some(), "and it still asks first");
    }

    #[test]
    fn a_cloned_repository_also_offers_working_in_the_checkout() {
        let mut app = with(vec![]);
        let repo = app.item_repo_key();
        // a real checkout, so its HEAD can be read
        app.clones_state = Load::Ready;
        app.clones
            .insert(repo, std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));

        let dests = app.dispatch_dests();
        let in_place: Vec<&Dest> = dests
            .iter()
            .filter(|d| {
                matches!(
                    d,
                    Dest::Fresh {
                        in_place: Some(_),
                        ..
                    }
                )
            })
            .collect();

        assert_eq!(
            in_place.len(),
            crate::shared::config::agent_kinds().len(),
            "one per agent, alongside the worktrees"
        );
        assert!(
            in_place[0].detail().contains("alongside your own work"),
            "and it warns that it is not isolated"
        );
        assert!(
            in_place[0].title().contains(", in the checkout"),
            "the title says where it lands: {}",
            in_place[0].title()
        );
    }

    #[test]
    fn working_in_the_checkout_makes_no_branch() {
        let mut app = with(vec![]);
        let repo = app.item_repo_key();
        app.clones_state = Load::Ready;
        app.clones
            .insert(repo, std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));

        let dests = app.dispatch_dests();
        let i = dests
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
            .expect("the checkout should be offered");
        app.dispatch_open = true;
        app.dispatch_sel = i;
        app.dispatch_accept();

        let plan = app.pending_fresh.expect("a plan");
        assert_eq!(plan.branch, None, "nothing is created on disk");
        assert_eq!(plan.repo_root, env!("CARGO_MANIFEST_DIR"));
    }

    #[test]
    fn the_worktree_is_offered_before_the_checkout() {
        // in place can collide with what the reader has open, so it should be
        // chosen rather than landed on
        let mut app = with(vec![]);
        let repo = app.item_repo_key();
        app.clones_state = Load::Ready;
        app.clones
            .insert(repo, std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));

        let dests = app.dispatch_dests();
        let first_worktree = dests
            .iter()
            .position(|d| matches!(d, Dest::Fresh { in_place: None, .. }));
        let first_in_place = dests.iter().position(|d| {
            matches!(
                d,
                Dest::Fresh {
                    in_place: Some(_),
                    ..
                }
            )
        });
        assert!(first_worktree < first_in_place);
    }

    #[test]
    fn choosing_a_running_agent_clears_any_earlier_worktree_plan() {
        // open the picker twice, pick a worktree then an agent: the stale plan
        // would otherwise turn the second send into a worktree
        let mut app = with(vec![agent("claude", AgentStatus::Idle, false)]);
        let repo = app.item_repo_key();
        scanned(&mut app, Some(&repo));

        app.dispatch_open = true;
        app.dispatch_sel = 1; // the first worktree, after the idle agent
        app.dispatch_accept();
        assert!(app.pending_fresh.is_some());

        app.cancel_prompt();
        app.dispatch_open = true;
        app.dispatch_sel = 0; // the agent
        app.dispatch_accept();
        assert!(app.pending_fresh.is_none(), "the plan did not survive");
    }

    #[test]
    fn cancelling_forgets_the_worktree_plan() {
        let mut app = with(vec![]);
        let repo = app.item_repo_key();
        scanned(&mut app, Some(&repo));
        app.dispatch_open = true;
        app.dispatch_accept();
        app.cancel_prompt();

        assert!(app.pending_fresh.is_none());
        assert!(app.prompt.is_none());
    }

    // --- what `x` sends depends on where you are standing ---

    #[test]
    fn the_subject_follows_the_tab_in_the_list() {
        use crate::github::subject::Subject;
        for (tab, want) in [(0, Subject::Issue), (1, Subject::Pr), (2, Subject::Run)] {
            let mut app = demo();
            app.tab = tab;
            app.view = View::List;
            assert_eq!(app.dispatch_subject(), Some(want), "tab {tab}");
        }
    }

    #[test]
    fn standing_in_a_log_sends_the_log_whatever_the_tab_says() {
        use crate::github::subject::Subject;
        let mut app = demo();
        app.tab = 1; // a pull request's checks lead here too
        app.view = View::Logs;
        assert_eq!(app.dispatch_subject(), Some(Subject::Run));
    }

    #[test]
    fn standing_in_a_diff_sends_that_file() {
        use crate::github::subject::Subject;
        let mut app = demo();
        app.tab = 1;
        app.view = View::Diff;
        assert_eq!(app.dispatch_subject(), Some(Subject::FileDiff));
    }

    #[test]
    fn a_pull_request_carries_its_files_not_only_its_body() {
        use crate::github::subject::Subject;
        let mut app = demo();
        app.tab = 1;
        app.view = View::Detail;

        let ctx = app.dispatch_context(Subject::Pr);
        assert!(
            ctx.contains("changed file"),
            "the file list is what makes a review actionable: {ctx}"
        );
    }

    #[test]
    fn a_run_carries_the_job_its_log_came_from() {
        use crate::github::subject::Subject;
        let mut app = demo();
        app.tab = 2;
        app.view = View::Logs;

        let ctx = app.dispatch_context(Subject::Run);
        assert!(
            ctx.starts_with("job: "),
            "an excerpt with no job named is hard to act on: {ctx}"
        );
    }

    #[test]
    fn each_subject_gets_its_own_wording() {
        use crate::github::subject::Subject;
        let issue = Subject::Issue.template();
        let run = Subject::Run.template();
        assert!(issue.starts_with("Work on"));
        assert!(run.starts_with("Diagnose"));
        assert_ne!(issue, run, "the first line is what an agent leans on");
    }

    #[test]
    fn the_picker_does_not_open_on_demo_data() {
        let mut app = demo();
        app.open_dispatch();
        assert!(!app.dispatch_open);
        assert!(app.flash.is_some(), "and it says why");
    }
}

// ------------------------------------------------------------- the explorer
//
// The tree arrives flat — a list of paths — and what is on screen is decided
// by which directories have been opened. That mapping is the whole feature.

mod explorer {
    use super::*;
    use crate::github::data::TreeEntry;

    fn entry(path: &str, is_dir: bool, size: u64) -> TreeEntry {
        TreeEntry {
            path: path.into(),
            is_dir,
            size,
        }
    }

    /// A small repository, in the order GitHub returns one: parents first.
    fn tree() -> Vec<TreeEntry> {
        vec![
            entry("README.md", false, 120),
            entry("src", true, 0),
            entry("src/main.rs", false, 900),
            entry("src/ui", true, 0),
            entry("src/ui/list.rs", false, 400),
            entry("tests", true, 0),
            entry("tests/e2e.rs", false, 200),
        ]
    }

    fn with_tree() -> App {
        let mut app = demo();
        app.tab = crate::github::data::FILES_TAB;
        app.view = View::List;
        let key = app.repo_key();
        app.trees.insert(key.clone(), tree());
        app.trees_state.insert(key, Load::Ready);
        app
    }

    fn paths(app: &App) -> Vec<String> {
        app.fs_rows().iter().map(|e| e.path.clone()).collect()
    }

    // --- what an entry knows about itself ---

    #[test]
    fn an_entry_knows_its_name_and_how_deep_it_sits() {
        let e = entry("src/ui/list.rs", false, 0);
        assert_eq!(e.name(), "list.rs");
        assert_eq!(e.depth(), 2);
    }

    #[test]
    fn a_top_level_entry_has_no_ancestors() {
        assert!(entry("README.md", false, 0).ancestors().is_empty());
    }

    #[test]
    fn ancestors_are_the_directories_above_it_outermost_first() {
        assert_eq!(
            entry("src/ui/list.rs", false, 0).ancestors(),
            vec!["src", "src/ui"]
        );
    }

    // --- what is on screen ---

    #[test]
    fn a_repository_opens_showing_its_top_level_and_nothing_more() {
        let app = with_tree();
        assert_eq!(paths(&app), vec!["README.md", "src", "tests"]);
    }

    #[test]
    fn opening_a_directory_reveals_its_children_but_not_its_grandchildren() {
        let mut app = with_tree();
        app.fs_open.insert("src".into());
        assert_eq!(
            paths(&app),
            vec!["README.md", "src", "src/main.rs", "src/ui", "tests"]
        );
    }

    #[test]
    fn a_child_stays_hidden_while_its_parent_is_closed() {
        // `src/ui` open but `src` closed: the grandchild must not leak out
        let mut app = with_tree();
        app.fs_open.insert("src/ui".into());
        assert!(!paths(&app).iter().any(|p| p == "src/ui/list.rs"));
    }

    #[test]
    fn enter_opens_a_directory_and_enter_again_closes_it() {
        let mut app = with_tree();
        app.pane = Pane::FileTree;
        app.fs_sel = 1; // src

        press(&mut app, KeyCode::Enter);
        assert!(app.fs_open.contains("src"));
        press(&mut app, KeyCode::Enter);
        assert!(!app.fs_open.contains("src"), "the same key closes it");
    }

    #[test]
    fn enter_on_a_file_moves_to_the_contents() {
        let mut app = with_tree();
        app.pane = Pane::FileTree;
        app.fs_sel = 0; // README.md

        press(&mut app, KeyCode::Enter);
        assert_eq!(app.pane, Pane::FileView);
        assert!(app.fs_open.is_empty(), "and opened no directory");
    }

    // --- filtering ---

    #[test]
    fn a_filter_reaches_files_no_directory_has_been_opened_for() {
        // the point of the filter: find a file without walking to it
        let mut app = with_tree();
        app.filter = "list".into();
        assert_eq!(paths(&app), vec!["src/ui/list.rs"]);
    }

    #[test]
    fn a_filter_matches_the_path_not_only_the_name() {
        let mut app = with_tree();
        app.filter = "tests/".into();
        assert_eq!(paths(&app), vec!["tests/e2e.rs"]);
    }

    #[test]
    fn a_filter_leaves_the_directories_out() {
        // a directory is not something you can read or send
        let mut app = with_tree();
        app.filter = "src".into();
        assert!(paths(&app).iter().all(|p| p.ends_with(".rs")));
    }

    // --- what can be read, and what cannot ---

    #[test]
    fn a_directory_is_not_a_file_to_fetch() {
        let mut app = with_tree();
        app.fs_sel = 1; // src
        assert_eq!(app.fs_selected_file(), None);
    }

    #[test]
    fn a_file_too_large_to_read_says_so_instead_of_being_fetched() {
        let mut app = with_tree();
        let key = app.repo_key();
        app.trees.insert(
            key,
            vec![entry("huge.bin", false, crate::github::gh::FILE_LIMIT + 1)],
        );
        app.fs_sel = 0;

        assert_eq!(app.fs_selected_file(), None, "it is never asked for");
        match app.file_body() {
            Err(st) => {
                let msg = st.error().unwrap_or_default();
                assert!(msg.contains("too large"), "{msg}");
                assert!(
                    !st.is_transient(),
                    "a file that is too large will be too large next time"
                );
            }
            Ok(_) => panic!("the reason should reach the pane"),
        }
    }

    // --- and sending it ---

    #[test]
    fn standing_in_the_explorer_sends_the_file() {
        use crate::github::subject::Subject;
        let mut app = with_tree();
        app.fs_sel = 0; // README.md
        assert_eq!(app.dispatch_subject(), Some(Subject::File));
    }

    #[test]
    fn a_directory_is_nothing_to_send() {
        let mut app = with_tree();
        app.fs_sel = 1; // src
        assert_eq!(app.dispatch_subject(), None);
    }

    #[test]
    fn the_picker_refuses_to_open_over_a_directory() {
        let mut app = with_tree();
        app.source = Source::Live;
        app.fs_sel = 1;
        app.open_dispatch();
        assert!(!app.dispatch_open);
        assert!(app.flash.is_some(), "and says why");
    }
}

// ---------------------------------------------------------- editing a file
//
// The explorer reads GitHub and an editor reads the disk. Those are the same
// file only when the local checkout is on the same branch, which on a real
// machine is often not the case — so most of what follows is about saying so.

mod editing {
    use super::*;
    use crate::github::actions::Prompt;
    use crate::github::data::TreeEntry;
    use std::path::PathBuf;

    /// A repository whose checkout really exists, so the path checks are real.
    fn at_file(rel: &str) -> App {
        let mut app = demo();
        app.source = Source::Live;
        app.tab = crate::github::data::FILES_TAB;
        app.view = View::List;
        app.pane = Pane::FileTree;

        let key = app.repo_key();
        app.trees.insert(
            key.clone(),
            vec![TreeEntry {
                path: rel.into(),
                is_dir: false,
                size: 100,
            }],
        );
        app.trees_state.insert(key.clone(), Load::Ready);
        app.clones_state = Load::Ready;
        app.clones
            .insert(key, PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        app
    }

    #[test]
    fn editing_asks_for_the_file_under_the_cursor_at_the_line_under_it() {
        let mut app = at_file("Cargo.toml");
        app.file_sel = 11; // zero-based

        app.open_in_editor();
        let (path, line) = app.edit_request.expect("the main loop should be asked");
        assert!(path.ends_with("Cargo.toml"), "{path:?}");
        assert_eq!(line, 12, "editors count from one");
    }

    #[test]
    fn a_file_the_checkout_does_not_have_is_not_opened() {
        // the usual reason: the local clone is on another branch
        let mut app = at_file("src/does-not-exist.rs");
        app.open_in_editor();

        assert!(app.edit_request.is_none());
        assert!(
            app.flash.is_some(),
            "and the reader is told, rather than nvim opening an empty buffer"
        );
    }

    #[test]
    fn a_directory_is_not_something_to_edit() {
        let mut app = at_file("src");
        if let Some(t) = app.trees.get_mut(&app.repo_key()) {
            t[0].is_dir = true;
        }
        app.open_in_editor();
        assert!(app.edit_request.is_none());
    }

    #[test]
    fn a_repository_that_is_not_here_is_offered_for_cloning() {
        let mut app = at_file("Cargo.toml");
        app.clones.clear();

        app.open_in_editor();
        assert!(app.edit_request.is_none(), "nothing to open yet");
        match app.prompt {
            Some(Prompt::Clone { ref repo, ref dest }) => {
                assert_eq!(*repo, app.repo_key());
                assert!(!dest.is_empty(), "somewhere to put it");
            }
            _ => panic!("it should ask before fetching a whole repository"),
        }
    }

    #[test]
    fn nothing_is_offered_while_the_disk_is_still_being_walked() {
        // an empty index and an unfinished scan look alike; offering to clone
        // something that is already here would be the wrong answer
        let mut app = at_file("Cargo.toml");
        app.clones.clear();
        app.clones_state = Load::Loading;

        app.open_in_editor();
        assert!(app.prompt.is_none());
        assert!(app.edit_request.is_none());
    }

    #[test]
    fn pressing_edit_before_the_disk_is_walked_asks_for_the_walk() {
        // The deadlock this exists for: it used to report that it was looking
        // for a checkout while nothing had asked anything to look.
        let mut app = at_file("Cargo.toml");
        app.clones.clear();
        app.clones_state = Load::Loading;

        app.open_in_editor();
        assert!(app.wants_edit, "the intent is remembered");
        assert!(app.edit_request.is_none(), "nothing to open yet");
    }

    #[test]
    fn the_editor_opens_once_the_walk_comes_back() {
        use crate::github::service::Response;

        let mut app = at_file("Cargo.toml");
        let repo = app.repo_key();
        app.clones.clear();
        app.clones_state = Load::Idle;
        app.open_in_editor();
        assert!(app.wants_edit);

        let mut index = crate::shared::clones::Index::new();
        index.insert(repo, std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        app.apply(Response::Scanned { index });

        assert!(
            app.edit_request.is_some(),
            "the keypress should not have been swallowed"
        );
        assert!(!app.wants_edit, "and the intent is spent");
    }

    #[test]
    fn a_walk_that_finds_nothing_offers_to_clone_rather_than_waiting_forever() {
        use crate::github::service::Response;

        let mut app = at_file("Cargo.toml");
        app.clones.clear();
        app.clones_state = Load::Idle;
        app.open_in_editor();

        app.apply(Response::Scanned {
            index: crate::shared::clones::Index::new(),
        });
        assert!(
            matches!(
                app.prompt,
                Some(crate::github::actions::Prompt::Clone { .. })
            ),
            "the second attempt reaches a real answer"
        );
    }

    #[test]
    fn editing_does_nothing_outside_the_file_tab() {
        let mut app = at_file("Cargo.toml");
        app.tab = 0;
        app.open_in_editor();
        assert!(app.edit_request.is_none());
    }

    #[test]
    fn the_cursor_moves_by_line_and_stops_at_the_ends() {
        let mut app = at_file("Cargo.toml");
        app.pane = Pane::FileView;
        let key = (app.repo_key(), "Cargo.toml".to_string());
        app.file_text.insert(key.clone(), "a\nb\nc\n".into());
        app.file_state.insert(key, Load::Ready);

        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.file_sel, 1);
        for _ in 0..10 {
            press(&mut app, KeyCode::Char('j'));
        }
        assert_eq!(app.file_sel, 2, "three lines, so index two is the last");
        for _ in 0..10 {
            press(&mut app, KeyCode::Char('k'));
        }
        assert_eq!(app.file_sel, 0);
    }

    #[test]
    fn moving_to_another_file_starts_at_its_top() {
        let mut app = at_file("Cargo.toml");
        app.file_sel = 40;
        app.select_in(Pane::FileTree, 0);
        assert_eq!(app.file_sel, 0);
    }
}

// -------------------------------------------------- what the last frame left
//
// A pane that does not paint every cell it owns leaves the previous frame's
// text showing through. It is invisible in a single snapshot and obvious the
// moment two frames differ, so these draw twice into the same terminal.

mod residue {
    use super::*;
    use crate::github::data::TreeEntry;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn entry(path: &str, size: u64) -> TreeEntry {
        TreeEntry {
            path: path.into(),
            is_dir: false,
            size,
        }
    }

    /// Two files: one with lines that reach the right edge, one with short
    /// ones. Switching between them is what exposes an unpainted cell.
    fn app_with_files() -> App {
        let mut app = demo();
        app.source = Source::Live;
        app.tab = crate::github::data::FILES_TAB;
        app.view = View::List;
        app.pane = Pane::FileTree;

        let key = app.repo_key();
        app.trees.insert(
            key.clone(),
            vec![entry("long.txt", 400), entry("short.txt", 40)],
        );
        app.trees_state.insert(key.clone(), Load::Ready);

        let long = (0..40)
            .map(|i| format!("{i} DISTINCTIVE_MARKER_TEXT_THAT_REACHES_THE_RIGHT_EDGE_OF_THE_PANE"))
            .collect::<Vec<_>>()
            .join("\n");
        app.file_text.insert((key.clone(), "long.txt".into()), long);
        app.file_state
            .insert((key.clone(), "long.txt".into()), Load::Ready);

        let short = (0..40).map(|_| "x").collect::<Vec<_>>().join("\n");
        app.file_text
            .insert((key.clone(), "short.txt".into()), short);
        app.file_state
            .insert((key, "short.txt".into()), Load::Ready);
        app
    }

    /// Everything on screen, as one string per row.
    fn rows(term: &Terminal<TestBackend>) -> Vec<String> {
        let buf = term.backend().buffer();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf.cell((x, y)).map_or(" ", |c| c.symbol()).to_string())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn a_shorter_file_does_not_leave_the_longer_one_showing_through() {
        let mut term = Terminal::new(TestBackend::new(120, 24)).unwrap();
        let mut app = app_with_files();

        app.fs_sel = 0; // the long one
        term.draw(|f| crate::github::ui::draw(f, &mut app)).unwrap();
        assert!(
            rows(&term).iter().any(|r| r.contains("DISTINCTIVE_MARKER")),
            "the first file should be on screen at all"
        );

        app.select_in(Pane::FileTree, 1); // the short one
        term.draw(|f| crate::github::ui::draw(f, &mut app)).unwrap();

        let after: Vec<String> = rows(&term);
        let leftover: Vec<&String> = after
            .iter()
            .filter(|r| r.contains("DISTINCTIVE") || r.contains("MARKER"))
            .collect();
        assert!(
            leftover.is_empty(),
            "the previous file is still on screen:\n{}",
            leftover
                .iter()
                .map(|r| format!("  {}", r.trim_end()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn a_forced_repaint_is_the_main_loops_to_make() {
        // Ratatui writes only what differs between two buffers, so nothing
        // inside a frame can repaint a cell it believes is already right.
        // `^l` therefore only raises a flag; the loop does the work.
        let mut app = demo();
        assert!(!app.wants_redraw);
        app.on_key(KeyEvent {
            code: KeyCode::Char('l'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        assert!(app.wants_redraw);
    }

    #[test]
    fn a_plain_l_still_moves_right_rather_than_repainting() {
        let mut app = demo_with_sidebar();
        app.pane = Pane::Repos;
        press(&mut app, KeyCode::Char('l'));
        assert!(!app.wants_redraw);
        assert_ne!(app.pane, Pane::Repos, "the pane key is untouched");
    }

    #[test]
    fn a_file_still_loading_does_not_show_the_last_one_underneath() {
        // the skeleton draws bars over some rows; the rest of the pane has to
        // be painted too, or the previous file reads as this one
        let mut term = Terminal::new(TestBackend::new(120, 24)).unwrap();
        let mut app = app_with_files();

        app.fs_sel = 0;
        term.draw(|f| crate::github::ui::draw(f, &mut app)).unwrap();

        app.select_in(Pane::FileTree, 1);
        let key = (app.repo_key(), "short.txt".to_string());
        app.file_text.remove(&key);
        app.file_state.insert(key, Load::Loading);
        term.draw(|f| crate::github::ui::draw(f, &mut app)).unwrap();

        let leftover: Vec<String> = rows(&term)
            .into_iter()
            .filter(|r| r.contains("DISTINCTIVE") || r.contains("MARKER"))
            .collect();
        assert!(
            leftover.is_empty(),
            "the previous file shows under the skeleton:\n{}",
            leftover.join("\n")
        );
    }
}

// --------------------------------------------------- a specific instruction
//
// The templates say the standard thing. Sometimes the standard thing is not
// what you want said, and typing beats editing a config file for that.

mod note {
    use super::*;
    use crate::shared::mux::AgentStatus;

    fn open_picker() -> App {
        let mut app = demo();
        app.source = Source::Live;
        app.agents = vec![crate::shared::mux::Agent {
            kind: "claude".into(),
            status: AgentStatus::Idle,
            cwd: "/home/x/orca/thing".into(),
            pane: "wA:p1".into(),
            title: String::new(),
            focused: false,
        }];
        app.open_dispatch();
        app
    }

    fn typed(app: &mut App, text: &str) {
        for c in text.chars() {
            press(app, KeyCode::Char(c));
        }
    }

    #[test]
    fn typing_in_the_picker_writes_an_instruction() {
        let mut app = open_picker();
        typed(&mut app, "only the parser");
        assert_eq!(app.dispatch_note, "only the parser");
    }

    #[test]
    fn the_letters_that_move_elsewhere_are_letters_here() {
        // `j` and `k` walk lists everywhere else in this program; inside the
        // picker they are just letters, which is why the arrows do the moving
        let mut app = open_picker();
        typed(&mut app, "jk");
        assert_eq!(app.dispatch_note, "jk");
        assert_eq!(app.dispatch_sel, 0, "nothing moved");
    }

    #[test]
    fn the_arrows_move_without_typing() {
        let mut app = open_picker();
        // a second destination, or there is nowhere to move to
        let mut other = app.agents[0].clone();
        other.pane = "wB:p1".into();
        app.agents.push(other);
        press(&mut app, KeyCode::Down);
        assert!(app.dispatch_sel > 0);
        assert!(app.dispatch_note.is_empty());
        press(&mut app, KeyCode::Up);
        assert_eq!(app.dispatch_sel, 0);
    }

    #[test]
    fn backspace_takes_a_character_back() {
        let mut app = open_picker();
        typed(&mut app, "abc");
        press(&mut app, KeyCode::Backspace);
        assert_eq!(app.dispatch_note, "ab");
    }

    #[test]
    fn an_instruction_leads_the_message_that_is_sent() {
        let mut app = open_picker();
        typed(&mut app, "only the parser, ignore the tests");
        app.dispatch_accept();

        match app.prompt {
            Some(Prompt::Dispatch { ref text, .. }) => {
                assert!(
                    text.starts_with("only the parser, ignore the tests"),
                    "the specific thing is what an agent reads first:\n{text}"
                );
                assert!(text.contains('#'), "and the template still follows it");
            }
            _ => panic!("nothing was queued"),
        }
    }

    #[test]
    fn no_instruction_sends_exactly_what_it_sent_before() {
        let mut app = open_picker();
        // whatever the fixture happens to be showing; the point is that the
        // message begins where the template begins
        let subject = app.dispatch_subject().expect("something to send");
        let template = subject.template();
        let opening = template.split('{').next().unwrap_or_default().to_string();
        assert!(!opening.is_empty(), "every template opens with words");

        app.dispatch_accept();
        match app.prompt {
            Some(Prompt::Dispatch { ref text, .. }) => {
                assert!(
                    text.starts_with(&opening),
                    "the template, untouched — expected it to open with {opening:?}:\n{text}"
                );
            }
            _ => panic!("nothing was queued"),
        }
    }

    #[test]
    fn the_instruction_does_not_outlive_the_question_it_was_for() {
        let mut app = open_picker();
        typed(&mut app, "just this once");
        app.dispatch_accept();

        app.open_dispatch();
        assert!(app.dispatch_note.is_empty(), "a specific thing is specific");
    }

    #[test]
    fn escape_still_closes_the_picker_rather_than_typing() {
        let mut app = open_picker();
        press(&mut app, KeyCode::Esc);
        assert!(!app.dispatch_open);
    }
}

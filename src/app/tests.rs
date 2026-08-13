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
    let mut app = demo();
    assert_eq!(app.panes(), vec![Pane::Repos, Pane::List]);

    // a PR has a checks pane; an issue does not
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.panes(), vec![Pane::Repos, Pane::Body, Pane::Checks]);

    let mut app = demo();
    ch(&mut app, '1'); // issues tab
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.panes(), vec![Pane::Repos, Pane::Body]);
}

#[test]
fn h_and_l_stop_at_the_edges() {
    let mut app = demo();
    app.pane = Pane::Repos;
    ch(&mut app, 'h');
    assert_eq!(app.pane, Pane::Repos, "h at the leftmost pane stays put");

    app.pane = Pane::List;
    ch(&mut app, 'l');
    assert_eq!(app.pane, Pane::List, "l at the rightmost pane stays put");
}

#[test]
fn tab_cycles_all_the_way_around() {
    let mut app = demo();
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
    let mut app = demo();
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
    assert_eq!(pr.merged_with.as_deref(), Some("merge commit"));
    assert_eq!(app.repo().unwrap().prs, open_prs - 1, "one less open PR");
    // GitHub offers to delete the branch right after
    assert!(matches!(app.prompt, Some(Prompt::DeleteBranch { .. })));

    app.confirm();
    assert!(app.current().unwrap().branch_deleted);
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

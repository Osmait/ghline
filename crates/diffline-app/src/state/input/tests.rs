//! Tests for diffline key handling, motions, queue, and modals.

use super::modals::looks_like_a_definition;
use super::*;
use crate::app::FinderTab;
use crate::model::State;
use crate::shared::nav::step;

#[test]
fn a_declaring_line_is_recognised_and_a_call_is_not() {
    assert!(looks_like_a_definition("pub fn parse(x: &str) {"));
    assert!(looks_like_a_definition("  export class TtlCache {"));
    assert!(looks_like_a_definition("func main() {"));
    assert!(!looks_like_a_definition("  return parse(x);"));
    assert!(!looks_like_a_definition(""));
}

#[test]
fn h_and_l_move_the_code_rather_than_the_focus() {
    // The complaint this answers: `l` inside the diff walked out to the
    // comment pane, so there was no way to reach the right-hand end of a
    // line that had been cut off.
    let mut a = app();
    a.pane = Pane::Diff;
    press(&mut a, Key::Char('l'));
    assert_eq!(a.pane, Pane::Diff, "focus should not have moved");
    assert!(a.hscroll > 0, "the code should have scrolled");

    press(&mut a, Key::Char('0'));
    assert_eq!(a.hscroll, 0, "0 returns to the start of the line");
}

#[test]
fn h_at_the_start_of_the_line_still_leaves_for_the_tree() {
    // Scrolled all the way back there is nothing left to scroll, so the
    // old habit keeps working rather than silently doing nothing.
    let mut a = app();
    a.pane = Pane::Diff;
    a.hscroll = 0;
    press(&mut a, Key::Char('h'));
    assert_eq!(a.pane, Pane::Tree);
}

#[test]
fn the_code_does_not_scroll_past_its_longest_line() {
    let mut a = app();
    a.pane = Pane::Diff;
    for _ in 0..200 {
        press(&mut a, Key::Char('l'));
    }
    assert_eq!(
        a.hscroll,
        a.max_hscroll(),
        "a pane of blank columns reads as a broken program"
    );
}

fn bracket(a: &mut App, open: char, c: char) {
    press(a, Key::Char(open));
    press(a, Key::Char(c));
}

fn leader(a: &mut App, c: char) {
    press(a, Key::Char(' '));
    press(a, Key::Char(c));
}

#[test]
fn a_note_in_the_tree_asks_about_the_file_rather_than_a_line() {
    let mut a = app();
    a.pane = Pane::Tree;
    leader(&mut a, 'n');
    assert_eq!(a.modal, Some(Modal::Comment));
    typed(&mut a, "why is this here?");
    press(&mut a, Key::Enter);

    assert_eq!(a.comments.len(), 1);
    let c = &a.comments[0];
    assert!(c.is_file_note(), "no line was selected, and none is meant");
    assert_eq!(c.path(), "src/a.rs");
    assert_eq!(c.body, "why is this here?");
    // and it says which file in the message the agent gets
    assert!(
        a.render_queue().contains("a.rs · the file"),
        "{}",
        a.render_queue()
    );
}

#[test]
fn a_file_note_does_not_take_the_lines_the_cursor_happened_to_be_on() {
    // The cursor is still somewhere in the diff while the tree has focus.
    // A note about the file must not quietly pick that up.
    let mut a = app();
    a.pane = Pane::Diff;
    a.cursor = 1;
    a.anchor = Some(3);
    assert!(
        !a.selected_anchors().is_empty(),
        "the fixture needs a selection"
    );

    a.pane = Pane::Tree;
    leader(&mut a, 'n');
    typed(&mut a, "about the file");
    press(&mut a, Key::Enter);
    assert!(a.comments[0].anchors.is_empty());
}

#[test]
fn the_toggle_focuses_before_it_hides() {
    // Hiding a pane you were only looking at is never what the key meant.
    let mut a = app();
    a.pane = Pane::Diff;
    assert!(a.tree_shown);
    leader(&mut a, 'e');
    assert!(a.tree_shown, "still there");
    assert_eq!(a.pane, Pane::Tree, "and now you are in it");
    leader(&mut a, 'e');
    assert!(!a.tree_shown, "a second press puts it away");
}

#[test]
fn the_queue_starts_away_and_the_tree_starts_open() {
    let a = app();
    assert!(a.tree_shown, "which files changed is the first question");
    assert!(
        !a.queue_shown,
        "empty until there is something to put in it"
    );
}

#[test]
fn the_leader_shows_and_hides_a_side_pane() {
    let mut a = app();
    a.pane = Pane::Diff;

    leader(&mut a, 'c');
    assert!(a.queue_shown);
    assert_eq!(a.pane, Pane::Queue, "asking for it means going to it");

    leader(&mut a, 'c');
    assert!(!a.queue_shown);
    assert_eq!(a.pane, Pane::Diff, "focus must not stay on what is gone");

    // the tree is already on screen, so the first press goes to it and
    // the second is the one that puts it away
    leader(&mut a, 'e');
    assert!(a.tree_shown);
    leader(&mut a, 'e');
    assert!(!a.tree_shown);
    leader(&mut a, 'd');
    assert_eq!(a.pane, Pane::Diff);
}

#[test]
fn focus_cannot_walk_onto_a_pane_that_is_not_drawn() {
    // The whole reason `panes()` is computed rather than fixed: `h` from
    // the code with the tree hidden used to land on a pane with nothing
    // on screen, and every key after that went somewhere invisible.
    let mut a = app();
    a.tree_shown = false;
    a.queue_shown = false;
    a.pane = Pane::Diff;
    a.hscroll = 0;
    press(&mut a, Key::Char('h'));
    assert_eq!(a.pane, Pane::Diff);
    press(&mut a, Key::Tab);
    assert_eq!(a.pane, Pane::Diff);
}

#[test]
fn a_leader_that_leads_nowhere_does_nothing_and_lets_go() {
    let mut a = app();
    a.pane = Pane::Diff;
    press(&mut a, Key::Char(' '));
    press(&mut a, Key::Char('z'));
    assert_eq!(a.pane, Pane::Diff);
    assert_eq!(a.pending, Pending::None, "the leader must not stay held");
    // and the very next key is a plain key again
    press(&mut a, Key::Char('l'));
    assert!(a.hscroll > 0);
}

fn agent_at(
    kind: &str,
    status: crate::shared::mux::AgentStatus,
    focused: bool,
    cwd: &str,
) -> crate::shared::mux::Agent {
    crate::shared::mux::Agent {
        kind: kind.into(),
        pane: format!("w:{kind}"),
        cwd: cwd.into(),
        status,
        title: String::new(),
        focused,
    }
}

#[test]
fn the_agent_showing_you_the_diff_is_not_a_target() {
    // Started from inside herdr, diffline is running in one of the very
    // panes it lists. Handing a review to yourself does nothing, and it
    // is an easy mistake when that agent is at the top of the list.
    use crate::shared::mux::AgentStatus;
    let mut a = app();
    let me = agent_at("claude", AgentStatus::Idle, true, "/tmp/r");
    assert!(
        a.refusal(&me).is_some(),
        "an idle agent is still no target when it is this window"
    );
    a.agents = vec![me];
    assert_eq!(a.first_usable_agent(), None);
}

#[test]
fn a_busy_agent_is_refused_with_a_reason_and_a_way_out() {
    use crate::shared::mux::AgentStatus;
    let a = app();
    let busy = agent_at("pi", AgentStatus::Working, false, "/tmp/r");
    let why = a.refusal(&busy).unwrap_or_default();
    assert!(why.contains("context"), "{why}");
    assert!(
        a.refusal(&agent_at("pi", AgentStatus::Idle, false, "/tmp/r"))
            .is_none()
    );
}

#[test]
fn the_first_usable_agent_prefers_one_in_this_repository() {
    // An agent working somewhere else has none of the files the review is
    // about.
    use crate::shared::mux::AgentStatus;
    let mut a = app();
    a.agents = vec![
        agent_at("pi", AgentStatus::Idle, false, "/somewhere/else"),
        agent_at("codex", AgentStatus::Idle, false, "/tmp/r"),
    ];
    assert_eq!(a.first_usable_agent(), Some(1), "the one that is here");
}

#[test]
fn a_busy_first_agent_does_not_become_the_default_target() {
    // `agent_idx` starts at 0, which is whatever the multiplexer listed
    // first — often this very window, or something busy elsewhere.
    use crate::shared::mux::AgentStatus;
    let mut a = app();
    a.agents = vec![
        agent_at("claude", AgentStatus::Working, true, "/tmp/r"),
        agent_at("codex", AgentStatus::Idle, false, "/tmp/r"),
    ];
    a.agent_idx = 0;
    press(&mut a, Key::Char(' '));
    press(&mut a, Key::Char('a'));
    assert_eq!(a.agent_idx, 1, "the picker landed on one worth choosing");
}

#[test]
fn a_new_agent_can_be_the_target_when_none_is_running() {
    // The complaint this answers: the queue could only go to an agent that
    // was already up, so a review with nothing running had nowhere to go.
    let mut a = app();
    assert!(a.agents.is_empty());
    let choices = a.agent_choices();
    assert!(
        !choices.is_empty(),
        "with nothing running there must still be something to pick"
    );
    assert!(choices.iter().all(|(_, is_new)| *is_new));

    a.sel = 0;
    let kind = choices[0].0.clone();
    a.modal = Some(Modal::Agents);
    press(&mut a, Key::Enter);
    assert_eq!(a.new_kind.as_deref(), Some(kind.as_str()));
}

#[test]
fn j_and_k_move_in_the_agent_picker() {
    // They did not: the picker has no query, but `j` fell through to the
    // branch that types into one, which also reset the selection — so the
    // list could only be walked with the arrow keys, and every letter
    // silently sent you back to the top.
    let mut a = app();
    a.modal = Some(Modal::Agents);
    a.sel = 0;
    press(&mut a, Key::Char('j'));
    assert_eq!(a.sel, 1);
    press(&mut a, Key::Char('k'));
    assert_eq!(a.sel, 0);
    press(&mut a, Key::Char('z'));
    assert_eq!(a.sel, 0, "a letter with nothing to do must not move it");
    assert!(a.query.is_empty(), "there is no query here to type into");
}

#[test]
fn picking_a_running_agent_clears_a_pending_new_one() {
    let mut a = app();
    a.agents = vec![crate::shared::mux::Agent {
        kind: "claude".into(),
        pane: "w:1".into(),
        cwd: "/tmp/r".into(),
        status: crate::shared::mux::AgentStatus::Idle,
        title: String::new(),
        focused: false,
    }];
    a.new_kind = Some("pi".into());
    a.modal = Some(Modal::Agents);
    a.sel = 0;
    press(&mut a, Key::Enter);
    assert_eq!(a.agent_idx, 0);
    assert!(
        a.new_kind.is_none(),
        "two targets at once would send the queue twice or nowhere"
    );
}

/// `src/` (a.rs, b.rs) and `README.md`, with the tree focused on a.rs.
fn in_the_tree() -> App {
    let mut a = app();
    a.files.push(ChangedFile {
        path: "README.md".into(),
        status: Status::Modified,
        add: 1,
        del: 1,
    });
    a.pane = Pane::Tree;
    a
}

fn labels(a: &App) -> Vec<String> {
    a.tree_rows().into_iter().map(|r| r.label).collect()
}

#[test]
fn the_changed_files_are_a_tree_with_folders_first() {
    let a = in_the_tree();
    assert_eq!(labels(&a), ["src", "a.rs", "b.rs", "README.md"]);
}

#[test]
fn h_folds_the_folder_a_file_is_in_and_l_opens_it_again() {
    let mut a = in_the_tree();
    press(&mut a, Key::Char('h'));
    assert_eq!(labels(&a), ["src", "README.md"]);
    assert_eq!(a.pane, Pane::Tree, "folding is not leaving the tree");
    assert_eq!(a.file_idx, 0, "the diff keeps the file it was showing");
    press(&mut a, Key::Char('l'));
    assert_eq!(labels(&a).len(), 4);
    assert_eq!(a.pane, Pane::Tree);
}

#[test]
fn capital_o_folds_every_folder_then_opens_every_one() {
    let mut a = in_the_tree();
    press(&mut a, Key::Char('O'));
    assert_eq!(labels(&a), ["src", "README.md"]);
    press(&mut a, Key::Char('O'));
    assert_eq!(labels(&a).len(), 4);
}

#[test]
fn enter_on_a_folder_folds_it_and_on_a_file_opens_the_diff() {
    let mut a = in_the_tree();
    press(&mut a, Key::Char('k'));
    press(&mut a, Key::Enter);
    assert_eq!(labels(&a), ["src", "README.md"]);
    assert_eq!(a.pane, Pane::Tree);
    press(&mut a, Key::Enter);
    press(&mut a, Key::Char('j'));
    press(&mut a, Key::Enter);
    assert_eq!(a.pane, Pane::Diff);
    assert_eq!(a.file_idx, 0);
}

#[test]
fn the_next_file_is_the_next_in_the_tree() {
    let mut a = in_the_tree();
    a.file_idx = 1; // b.rs, whose next in the tree is README.md
    a.step_file(1);
    assert_eq!(a.path(), "README.md");
    a.step_file(1);
    assert_eq!(a.path(), "src/a.rs", "round the end to the start");
}

#[test]
fn stepping_stops_at_the_ends_and_survives_an_empty_list() {
    assert_eq!(step(0, -1, 5), 0);
    assert_eq!(step(4, 1, 5), 4);
    assert_eq!(step(0, 1, 0), 0, "nowhere to go");
}

use crate::app::Load;
use crate::model::{ChangedFile, Kind, Row, Scope, Status};
fn press(app: &mut App, key: Key) {
    app.on_key(Press::new(key));
}

fn typed(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, Key::Char(c));
    }
}

/// A diff already in place and no worker: these are about the reducer.
fn app() -> App {
    let mut a = App::new(
        "/tmp/r".into(),
        Scope::WorkingTree,
        vec![Scope::WorkingTree],
        None,
    );
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
        Row {
            kind: Kind::Header,
            old: None,
            new: None,
            text: "@@ -1,3 +1,4 @@".into(),
        },
        Row {
            kind: Kind::Context,
            old: Some(1),
            new: Some(1),
            text: "fn main() {".into(),
        },
        Row {
            kind: Kind::Deleted,
            old: Some(2),
            new: None,
            text: "  old();".into(),
        },
        Row {
            kind: Kind::Added,
            old: None,
            new: Some(2),
            text: "  new();".into(),
        },
        Row {
            kind: Kind::Added,
            old: None,
            new: Some(3),
            text: "  more();".into(),
        },
    ];
    a.rows.insert("src/a.rs".into(), rows);
    a.rows_state.insert("src/a.rs".into(), Load::Ready);
    a.cursor = 1;
    a
}

// --- what the audit of every command found ---

#[test]
fn esc_in_the_theme_picker_puts_back_the_theme_it_opened_on() {
    use crate::tui::theme::{Theme, current, set};
    set(Theme::Default);
    let mut a = app();
    a.run(Action::Themes, 1);
    press(&mut a, Key::Char('j'));
    assert_ne!(current(), Theme::Default, "moving previews");
    press(&mut a, Key::Esc);
    assert_eq!(current(), Theme::Default, "and esc undoes it");
    set(Theme::Default);
}

#[test]
fn ctrl_l_repaints_and_can_be_rebound() {
    let mut a = app();
    a.on_key(Press::ctrl(Key::Char('l')));
    assert!(a.wants_redraw, "the shipped keymap repaints on it");
    let mut b = app();
    b.keys = crate::keys::Map::with("<C-l> = none\n");
    b.on_key(Press::ctrl(Key::Char('l')));
    assert!(!b.wants_redraw, "and a keys file can take it away");
}

#[test]
fn the_help_scrolls_and_closes_only_on_the_keys_that_close() {
    let mut a = app();
    a.run(Action::Help, 1);
    press(&mut a, Key::Char('j'));
    assert_eq!(a.help_scroll, 1);
    press(&mut a, Key::Char('x'));
    assert_eq!(a.modal, Some(Modal::Help), "any key no longer closes it");
    press(&mut a, Key::Char('q'));
    assert_eq!(a.modal, None);
    a.run(Action::Help, 1);
    assert_eq!(a.help_scroll, 0, "and it opens at its top again");
}

#[test]
fn enter_on_a_line_of_the_diff_opens_a_note_on_it() {
    let mut a = app();
    a.pane = Pane::Diff;
    press(&mut a, Key::Enter);
    assert_eq!(a.modal, Some(Modal::Comment));
}

#[test]
fn gg_and_capital_g_move_the_tree_when_the_tree_has_the_focus() {
    let mut a = app();
    a.pane = Pane::Tree;
    press(&mut a, Key::Char('G'));
    assert_eq!(a.path(), "src/b.rs", "the last file of the tree");
    assert_eq!(
        a.cursor, 0,
        "and not the diff's cursor, which a new file resets"
    );
    typed(&mut a, "gg");
    // the top of the tree is the `src/` folder, and on a folder the diff
    // keeps the file it was showing
    assert_eq!(a.tree_row(&a.tree_rows()), 0);
    assert_eq!(a.tree_folds.dir.as_deref(), Some("src"));
}

#[test]
fn shift_tab_walks_the_finders_tabs_backwards() {
    let mut a = app();
    a.run(Action::Search, 1);
    let first = a.finder_tab;
    press(&mut a, Key::BackTab);
    assert_ne!(a.finder_tab, first);
    press(&mut a, Key::Tab);
    assert_eq!(a.finder_tab, first);
}

#[test]
fn the_palette_offers_the_tree_the_queue_and_whitespace() {
    let mut a = app();
    assert!(a.run_command("ignore whitespace"));
    assert!(a.ignore_ws);
    assert!(a.run_command("fold or open all folders"));
    assert!(!a.tree_folds.folded.is_empty());
}

// --- searching every file ---

/// The fixture with a corpus for the whole scope: `src/b.rs`, never
/// opened, says `needle()` on its second row.
fn with_corpus() -> App {
    let mut a = app();
    let b = vec![
        Row {
            kind: Kind::Header,
            old: None,
            new: None,
            text: "@@ -1 +1,2 @@".into(),
        },
        Row {
            kind: Kind::Added,
            old: None,
            new: Some(1),
            text: "  needle();".into(),
        },
    ];
    a.corpus
        .insert("src/a.rs".into(), a.rows["src/a.rs"].clone());
    a.corpus.insert("src/b.rs".into(), b);
    a.corpus_state = Load::Ready;
    a.corpus_for = Some((a.scope.clone(), a.opts()));
    a.last_search = "needle".into();
    a
}

#[test]
fn n_finds_a_match_in_a_file_that_was_never_opened() {
    let mut a = with_corpus();
    press(&mut a, Key::Char('n'));
    assert_eq!(a.path(), "src/b.rs");
    assert_eq!(a.row_pending, Some(1), "the row waits for the file's rows");

    // and lands when they do
    let rows = a.corpus["src/b.rs"].clone();
    a.apply(crate::service::Response::Diff {
        scope: a.scope.clone(),
        path: "src/b.rs".into(),
        opts: a.opts(),
        result: Ok((rows, Vec::new())),
    });
    assert_eq!(a.cursor, 1);
    assert_eq!(a.row_pending, None);
}

#[test]
fn a_search_with_no_corpus_waits_for_one_rather_than_saying_no_match() {
    let mut a = app();
    a.last_search = "needle".into();
    press(&mut a, Key::Char('n'));
    assert_eq!(a.search_pending, Some(1));
    assert_eq!(a.path(), "src/a.rs", "nothing moved yet");
}

#[test]
fn n_round_the_end_comes_back_to_the_open_file() {
    let mut a = with_corpus();
    a.last_search = "main".into(); // only in src/a.rs, above the cursor
    a.cursor = 3;
    press(&mut a, Key::Char('n'));
    assert_eq!(a.path(), "src/a.rs");
    assert_eq!(a.cursor, 1);
}

#[test]
fn the_finder_reads_lines_of_files_never_opened() {
    let mut a = with_corpus();
    a.finder_tab = crate::app::FinderTab::Grep;
    a.query = "needle".into();
    let hits = a.hits();
    assert!(
        hits.iter().any(|h| h.file == 1),
        "src/b.rs, from the corpus"
    );
}

#[test]
fn a_new_rule_lets_the_corpus_go() {
    let mut a = with_corpus();
    a.toggle_whitespace();
    assert!(!a.corpus_ready());
    assert!(a.corpus.is_empty());
}

// --- moving ---

#[test]
fn the_cursor_never_lands_on_a_hunk_header() {
    let mut a = app();
    press(&mut a, Key::Char('k'));
    assert_eq!(a.cursor, 1, "it stopped rather than sitting on the @@ line");
    assert!(a.diff_rows()[a.cursor].kind.is_code());
}

#[test]
fn the_cursor_stops_at_the_end() {
    let mut a = app();
    for _ in 0..20 {
        press(&mut a, Key::Char('j'));
    }
    assert_eq!(a.cursor, a.diff_rows().len() - 1);
}

#[test]
fn gg_takes_two_presses_and_g_then_j_is_a_j() {
    let mut a = app();
    a.cursor = 4;
    press(&mut a, Key::Char('g'));
    assert_eq!(a.cursor, 4, "one g does nothing yet");
    press(&mut a, Key::Char('g'));
    assert_eq!(a.cursor, 1, "the second one goes to the top");

    a.cursor = 3;
    press(&mut a, Key::Char('g'));
    press(&mut a, Key::Char('j'));
    assert_eq!(a.cursor, 4, "the pending g did not swallow the j");
}

fn ctrl(a: &mut App, c: char) {
    a.on_key(Press::ctrl(Key::Char(c)));
}

#[test]
fn a_rebound_key_does_the_new_thing_and_the_old_key_does_nothing() {
    let mut a = app();
    a.keys = crate::keys::Map::with("s = split\nj = none\n<C-n> = line-down\n");
    a.pane = Pane::Diff;

    assert!(!a.split);
    press(&mut a, Key::Char('s'));
    assert!(a.split, "s was bound to split");

    let before = a.cursor;
    press(&mut a, Key::Char('j'));
    assert_eq!(a.cursor, before, "j was taken away");

    ctrl(&mut a, 'n');
    assert_ne!(a.cursor, before, "and ^n took its job");
}

#[test]
fn unbinding_everything_behind_a_prefix_gives_the_prefix_back() {
    // `]` swallowed the next key because it was hardcoded as a prefix.
    // Now it is one only while something lives behind it, so a reader who
    // clears them out can bind `]` to something itself.
    let mut a = app();
    a.keys = crate::keys::Map::with("]c = none\n]f = none\n]s = none\n] = line-down\n");
    a.pane = Pane::Diff;
    let before = a.cursor;
    press(&mut a, Key::Char(']'));
    assert_ne!(a.cursor, before, "] should have moved, not waited");
}

#[test]
fn a_count_survives_the_prefix_that_follows_it() {
    // `5gg` is one command. Consuming the count when `g` opened its
    // alphabet left the `gg` with nothing to act on, and the cursor went
    // to line 1 instead of line 5.
    let mut a = app();
    typed(&mut a, "3");
    assert_eq!(a.count, Some(3), "the digit is held, not acted on");
    press(&mut a, Key::Char('g'));
    assert_eq!(a.count, Some(3), "and it survives the prefix");
    press(&mut a, Key::Char('g'));

    let code: Vec<usize> = (0..a.diff_rows().len())
        .filter(|i| a.diff_rows()[*i].kind.is_code())
        .collect();
    assert_eq!(a.cursor, code[2], "the third code line");
}

#[test]
fn a_count_repeats_a_motion() {
    let mut a = app();
    a.cursor = 1;
    typed(&mut a, "2");
    press(&mut a, Key::Char('j'));
    let mut b = app();
    b.cursor = 1;
    press(&mut b, Key::Char('j'));
    press(&mut b, Key::Char('j'));
    assert_eq!(a.cursor, b.cursor, "2j is j twice");
    assert_eq!(a.count, None, "and the count is spent");
}

#[test]
fn a_digit_is_a_count_but_zero_alone_is_a_motion() {
    // `0` is the start of the line unless it is continuing a number,
    // which is the one piece of vim's digit handling that is not obvious.
    let mut a = app();
    a.pane = Pane::Diff;
    a.hscroll = 12;
    press(&mut a, Key::Char('0'));
    assert_eq!(a.hscroll, 0, "a bare 0 goes to the start of the line");

    a.hscroll = 12;
    typed(&mut a, "10");
    assert_eq!(a.count, Some(10), "after a 1 it is part of the number");
    assert_eq!(a.hscroll, 12, "and it did not move anything");
}

/// The fixture's `src/a.rs` with a second hunk after its first.
fn two_hunks() -> App {
    let mut a = app();
    let mut rows = a.rows["src/a.rs"].clone();
    rows.push(Row {
        kind: Kind::Header,
        old: None,
        new: None,
        text: "@@ -20,2 +21,2 @@".into(),
    });
    rows.push(Row {
        kind: Kind::Context,
        old: Some(20),
        new: Some(21),
        text: "fn later() {".into(),
    });
    a.rows.insert("src/a.rs".into(), rows);
    a.cursor = first_code(a.diff_rows(), 0);
    a
}

#[test]
fn a_brace_at_a_hunks_first_line_goes_to_the_hunk_before() {
    let mut a = two_hunks();
    press(&mut a, Key::Char('}'));
    assert_eq!(a.cursor, 6, "the second hunk's first line");
    press(&mut a, Key::Char('{'));
    assert_eq!(a.cursor, 1, "and back, rather than standing still");
}

#[test]
fn past_the_last_hunk_a_brace_goes_on_into_the_next_file() {
    let mut a = two_hunks();
    press(&mut a, Key::Char('}'));
    press(&mut a, Key::Char('}'));
    assert_eq!(a.path(), "src/b.rs");
    assert_eq!(a.pane, Pane::Diff, "still reading");
}

#[test]
fn above_the_first_hunk_a_brace_goes_back_to_the_file_before() {
    let mut a = app();
    a.goto_file(1);
    press(&mut a, Key::Char('{'));
    assert_eq!(a.path(), "src/a.rs");
}

#[test]
fn the_last_files_last_hunk_goes_nowhere_and_says_so() {
    let mut a = app();
    a.goto_file(1);
    press(&mut a, Key::Char('}'));
    assert_eq!(a.path(), "src/b.rs");
    assert_eq!(a.toast, "last file");
}

#[test]
fn hunks_and_changes_are_different_motions() {
    let mut a = app();
    a.cursor = first_code(a.diff_rows(), 0);

    // `}` goes to the next hunk; this fixture has one, so it carries on
    // into the next file
    press(&mut a, Key::Char('}'));
    assert_eq!(a.path(), "src/b.rs");

    // `]c` walks changed lines, and a run of them counts once
    let mut b = app();
    b.cursor = first_code(b.diff_rows(), 0);
    bracket(&mut b, ']', 'c');
    assert!(
        matches!(b.diff_rows()[b.cursor].kind, Kind::Added | Kind::Deleted),
        "]c should land on a change, landed on {:?}",
        b.diff_rows()[b.cursor].kind
    );
}

#[test]
fn screen_motions_are_about_the_window_not_the_file() {
    let mut a = app();
    a.view_height = 3;
    a.diff_scroll = 1;
    press(&mut a, Key::Char('L'));
    let low = a.cursor;
    press(&mut a, Key::Char('H'));
    assert!(a.cursor <= low, "H is above L");
    assert!(
        a.cursor >= a.diff_scroll,
        "H must not go above what is on screen"
    );
}

#[test]
fn zt_and_zb_move_the_window_and_leave_the_cursor() {
    let mut a = app();
    a.view_height = 4;
    a.cursor = 4;
    press(&mut a, Key::Char('z'));
    press(&mut a, Key::Char('t'));
    assert_eq!(a.diff_scroll, 4, "the cursor line is now the top one");
    assert_eq!(a.cursor, 4, "and the cursor did not move");
}

#[test]
fn ctrl_e_moves_the_window_and_drags_the_cursor_only_at_the_edge() {
    // It used to move only the scroll, which the next frame's
    // `scroll_into_view` sprang straight back — `^e` was a key that
    // flickered and did nothing.
    let mut a = app();
    a.view_height = 3;
    a.cursor = 1;
    ctrl(&mut a, 'e');
    assert_eq!(a.diff_scroll, 1, "the window answered at once");
    assert_eq!(a.cursor, 1, "the cursor still fit, so it stayed put");
    ctrl(&mut a, 'e');
    assert_eq!(a.diff_scroll, 2);
    assert_eq!(a.cursor, 2, "dragged just inside the top edge");
    ctrl(&mut a, 'e');
    assert_eq!(a.diff_scroll, 2, "there is no window past the last one");
}

#[test]
fn ctrl_y_at_the_top_leaves_everything_alone() {
    let mut a = app();
    a.view_height = 3;
    a.cursor = 1;
    ctrl(&mut a, 'y');
    assert_eq!(a.diff_scroll, 0);
    assert_eq!(a.cursor, 1);
}

#[test]
fn a_count_scrolls_the_view_that_many_lines() {
    // `2<C-e>` used to be a key wasted: the count was read and dropped.
    let mut a = app();
    a.view_height = 2;
    typed(&mut a, "2");
    ctrl(&mut a, 'e');
    assert_eq!(a.diff_scroll, 2);
}

#[test]
fn the_dragged_cursor_slides_off_a_hunk_header() {
    let ctx = |n: u32| Row {
        kind: Kind::Context,
        old: Some(n),
        new: Some(n),
        text: format!("line {n}"),
    };
    let hdr = || Row {
        kind: Kind::Header,
        old: None,
        new: None,
        text: "@@".into(),
    };
    let mut a = app();
    a.rows.insert(
        "src/a.rs".into(),
        vec![hdr(), ctx(1), ctx(2), hdr(), ctx(3), ctx(4)],
    );
    a.cursor = 1;
    a.view_height = 2;
    for _ in 0..3 {
        ctrl(&mut a, 'e');
    }
    assert_eq!(a.diff_scroll, 3, "the window may start on the @@ line");
    assert_eq!(a.cursor, 4, "the cursor must not");
}

#[test]
fn enter_on_a_queued_note_returns_to_its_lines() {
    let mut a = app();
    a.cursor = 3;
    leader(&mut a, 'n');
    typed(&mut a, "note here");
    press(&mut a, Key::Enter);

    a.cursor = 1;
    a.queue_shown = true;
    a.pane = Pane::Queue;
    press(&mut a, Key::Enter);
    assert_eq!(a.pane, Pane::Diff, "a note is read where it was made");
    assert_eq!(a.cursor, 3, "on the line it is about");
}

#[test]
fn ctrl_d_is_half_a_screen_of_whatever_size_it_is() {
    let mut a = app();
    a.view_height = 10;
    a.cursor = 1;
    ctrl(&mut a, 'd');
    let ten = a.cursor;
    a.cursor = 1;
    a.view_height = 2;
    ctrl(&mut a, 'd');
    assert!(
        a.cursor <= ten,
        "a shorter window should move less, not the same fixed ten"
    );
}

#[test]
fn n_repeats_what_slash_asked_for() {
    let mut a = app();
    press(&mut a, Key::Char('/'));
    typed(&mut a, "new");
    press(&mut a, Key::Esc);
    assert_eq!(a.last_search, "new", "escaping still leaves a last search");

    a.cursor = 1;
    press(&mut a, Key::Char('n'));
    assert!(
        a.diff_rows()[a.cursor].text.to_lowercase().contains("new"),
        "landed on {:?}",
        a.diff_rows()[a.cursor].text
    );
}

#[test]
fn a_motion_in_visual_mode_grows_the_selection() {
    // Visual mode is the whole point of the motions being separate from
    // the commands: everything that moves must extend a range too.
    let mut a = app();
    a.cursor = 1;
    press(&mut a, Key::Char('V'));
    typed(&mut a, "2");
    press(&mut a, Key::Char('j'));
    let (lo, hi) = a.span();
    assert!(a.visual());
    assert!(hi > lo, "the range should have grown");
    assert_eq!(lo, 1, "and kept the end it started from");
}

#[test]
fn o_swaps_the_ends_of_a_selection() {
    let mut a = app();
    a.cursor = 1;
    press(&mut a, Key::Char('V'));
    press(&mut a, Key::Char('j'));
    let (lo, hi) = a.span();
    press(&mut a, Key::Char('o'));
    assert_eq!(a.span(), (lo, hi), "the range is the same");
    assert_eq!(a.cursor, lo, "but the cursor is now at the other end");
}

#[test]
fn stepping_past_the_last_file_comes_back_to_the_first() {
    let mut a = app();
    bracket(&mut a, ']', 'f');
    assert_eq!(a.file_idx, 1);
    bracket(&mut a, ']', 'f');
    assert_eq!(a.file_idx, 0);
}

// --- the visual range ---

#[test]
fn v_opens_a_range_and_esc_closes_it() {
    let mut a = app();
    press(&mut a, Key::Char('V'));
    assert!(a.visual());
    press(&mut a, Key::Esc);
    assert!(!a.visual());
}

#[test]
fn a_range_grows_with_the_cursor() {
    let mut a = app();
    press(&mut a, Key::Char('V'));
    press(&mut a, Key::Char('j'));
    press(&mut a, Key::Char('j'));
    assert_eq!(a.span(), (1, 3));
    assert_eq!(a.selected_anchors().len(), 3);
}

// --- comments ---

#[test]
fn a_comment_is_written_against_the_line_it_was_made_on() {
    let mut a = app();
    leader(&mut a, 'n');
    typed(&mut a, "why here?");
    press(&mut a, Key::Enter);

    assert_eq!(a.comments.len(), 1);
    assert_eq!(a.comments[0].body, "why here?");
    assert_eq!(a.comments[0].where_label(), "a.rs:1");
}

#[test]
fn a_comment_on_a_range_holds_every_line_of_it() {
    let mut a = app();
    press(&mut a, Key::Char('V'));
    press(&mut a, Key::Char('j'));
    press(&mut a, Key::Char('j'));
    leader(&mut a, 'n');
    typed(&mut a, "this whole block");
    press(&mut a, Key::Enter);

    assert_eq!(a.comments[0].lines(), 3);
    assert!(!a.visual(), "and the range closed behind it");
}

#[test]
fn an_empty_note_is_not_a_comment() {
    let mut a = app();
    leader(&mut a, 'n');
    press(&mut a, Key::Enter);
    assert!(a.comments.is_empty());
    assert!(a.modal.is_none());
}

#[test]
fn escape_discards_the_draft_rather_than_saving_it() {
    let mut a = app();
    leader(&mut a, 'n');
    typed(&mut a, "never mind");
    press(&mut a, Key::Esc);
    assert!(a.comments.is_empty());
    assert!(a.draft.is_empty());
}

#[test]
fn x_removes_the_comment_the_cursor_is_inside() {
    let mut a = app();
    press(&mut a, Key::Char('V'));
    press(&mut a, Key::Char('j'));
    leader(&mut a, 'n');
    typed(&mut a, "x");
    press(&mut a, Key::Enter);
    assert_eq!(a.comments.len(), 1);

    // the cursor is on the second line of a two-line comment
    a.cursor = 2;
    leader(&mut a, 'x');
    assert!(a.comments.is_empty(), "being inside it is enough");
}

#[test]
fn a_comment_survives_the_diff_being_fetched_again() {
    // the whole point of anchoring to lines rather than to rows
    let mut a = app();
    leader(&mut a, 'n');
    typed(&mut a, "keep me");
    press(&mut a, Key::Enter);

    a.refresh();
    assert_eq!(a.comments.len(), 1);
    assert_eq!(a.comments[0].where_label(), "a.rs:1");
}

#[test]
fn changing_the_context_re_asks_only_for_the_open_file() {
    let mut a = app();
    a.rows.insert("src/b.rs".into(), a.diff_rows().to_vec());
    a.rows_state.insert("src/b.rs".into(), Load::Ready);
    leader(&mut a, '+');
    assert_eq!(a.context, 6);
    assert_eq!(a.rows_state.get("src/a.rs"), Some(&Load::Idle));
    assert!(
        !a.rows.contains_key("src/b.rs") && !a.rows_state.contains_key("src/b.rs"),
        "other cached files must be evicted so they re-fetch at ±6 when opened"
    );
}

#[test]
fn goto_screen_and_scroll_cursor_to_use_pair_coordinates_in_split_mode() {
    use crate::model::{Kind, Row};

    let mut a = app();
    a.split = true;
    a.view_height = 4;
    // 1 header + 4 paired del/add lines (= 4 pairs) + 2 context lines (= 2 pairs) -> 7 pairs, 11 rows
    let mut rows = vec![Row {
        kind: Kind::Header,
        old: None,
        new: None,
        text: "@@ -1,6 +1,6 @@".into(),
    }];
    for i in 1..=4 {
        rows.push(Row {
            kind: Kind::Deleted,
            old: Some(i),
            new: None,
            text: format!("old {i}"),
        });
    }
    for i in 1..=4 {
        rows.push(Row {
            kind: Kind::Added,
            old: None,
            new: Some(i),
            text: format!("new {i}"),
        });
    }
    for i in 5..=6 {
        rows.push(Row {
            kind: Kind::Context,
            old: Some(i),
            new: Some(i),
            text: format!("ctx {i}"),
        });
    }
    a.rows.insert("src/a.rs".into(), rows);
    // Row 10 is the last context line, which is pair index 6.
    a.cursor = 10;
    a.scroll_cursor_to(Place::Top);
    assert_eq!(a.diff_scroll, 6, "zt in split mode scrolls to pair index 6");

    a.diff_scroll = 1;
    a.goto_screen(Place::Top);
    // Pair 1 pairs Deleted row 1 (left) with Added row 5 (right); `goto_screen` prefers right.
    assert_eq!(a.cursor, 5);
}

#[test]
fn the_context_has_ends_and_stays_between_them() {
    let mut a = app();
    for _ in 0..20 {
        leader(&mut a, '+');
    }
    assert_eq!(a.context, 21);
    for _ in 0..20 {
        leader(&mut a, '-');
    }
    assert_eq!(a.context, 3);
}

// --- what an agent receives ---

#[test]
fn the_queue_travels_as_one_message_grouped_by_file() {
    let mut a = app();
    leader(&mut a, 'n');
    typed(&mut a, "first note");
    press(&mut a, Key::Enter);
    a.cursor = 3;
    leader(&mut a, 'n');
    typed(&mut a, "second note");
    press(&mut a, Key::Enter);

    let text = a.render_queue();
    assert!(text.contains("--- src/a.rs"), "grouped under its file");
    assert_eq!(text.matches("--- src/a.rs").count(), 1, "once, not twice");
    assert!(text.contains("first note") && text.contains("second note"));
    assert!(text.contains("a.rs:1") && text.contains("a.rs:2"));
    assert!(text.contains("working tree"), "the scope is named");
}

#[test]
fn notes_arrive_in_the_order_the_agent_will_work_in() {
    let mut a = app();
    a.cursor = 4; // the later line first
    leader(&mut a, 'n');
    typed(&mut a, "later");
    press(&mut a, Key::Enter);
    a.cursor = 1;
    leader(&mut a, 'n');
    typed(&mut a, "earlier");
    press(&mut a, Key::Enter);

    let text = a.render_queue();
    assert!(
        text.find("earlier") < text.find("later"),
        "line order, not the order they were written:\n{text}"
    );
}

#[test]
fn sending_an_empty_queue_says_so_rather_than_sending_nothing() {
    let mut a = app();
    leader(&mut a, 's');
    assert!(!a.busy);
    assert!(a.toast.contains("empty"), "{}", a.toast);
}

#[test]
fn sending_with_no_agent_says_which_key_finds_one() {
    let mut a = app();
    leader(&mut a, 'n');
    typed(&mut a, "x");
    press(&mut a, Key::Enter);
    leader(&mut a, 's');
    assert!(!a.busy);
    assert!(a.toast.contains('a'), "{}", a.toast);
}

#[test]
fn a_busy_agent_is_refused_with_its_reason() {
    let mut a = app();
    a.agents = vec![crate::shared::mux::Agent {
        kind: "claude".into(),
        status: crate::shared::mux::AgentStatus::Working,
        cwd: "/tmp/r".into(),
        pane: "wA:p1".into(),
        title: String::new(),
        focused: false,
    }];
    leader(&mut a, 'n');
    typed(&mut a, "x");
    press(&mut a, Key::Enter);
    leader(&mut a, 's');

    assert!(!a.busy, "nothing was sent");
    assert!(a.toast.contains("working"), "{}", a.toast);
    assert_eq!(a.comments[0].state, State::Queued, "and it stayed queued");
}

// --- the history ---

fn commit(fill: char, subject: &str, author: &str) -> crate::model::LogEntry {
    crate::model::LogEntry {
        sha: fill.to_string().repeat(40),
        subject: subject.into(),
        author: author.into(),
        when: 1_700_000_000,
    }
}

#[test]
fn the_history_opens_filters_and_reviews_a_commit() {
    use crate::service::{Request, Response};
    use crate::shared::worker::Immediate;

    let mut a = app();
    a.service = Some(Box::new(Immediate::new(|req| match req {
        Request::Log { .. } => Response::Log(Ok(vec![
            commit('b', "newer thing", "Maria"),
            commit('a', "fix the parser", "Luis"),
        ])),
        _ => Response::Sent(Ok(())),
    })));

    leader(&mut a, 'l');
    assert_eq!(a.modal, Some(Modal::History));
    a.ensure();
    while let Some(r) = a.poll() {
        a.apply(r);
    }
    assert_eq!(a.log.len(), 2, "opening asked for the log");

    typed(&mut a, "parser");
    assert_eq!(a.history_hits().len(), 1, "the query narrowed the list");
    press(&mut a, Key::Enter);

    assert_eq!(a.modal, None);
    assert_eq!(
        a.scope,
        Scope::Commit {
            sha: "a".repeat(40)
        },
        "the review is now of that commit"
    );
    assert_eq!(a.files_state, Load::Idle, "and its files were asked for");
    assert!(a.toast.contains("fix the parser"), "{}", a.toast);
}

#[test]
fn the_history_searches_author_and_sha_too() {
    let mut a = app();
    a.log = vec![
        commit('b', "one thing", "Maria Okonkwo"),
        commit('a', "another", "Luis Serrano"),
    ];
    a.query = "maria".into();
    assert_eq!(a.history_hits(), vec![0]);
    a.query = "aaaaaaa".into();
    assert_eq!(a.history_hits(), vec![1], "the short sha is searchable");
}

#[test]
fn picking_again_retargets_the_commit_tab_rather_than_growing_one() {
    // `[s`/`]s` walk the places a review can stand; a trail of every
    // commit ever visited would make the cycle unbounded.
    let mut a = app();
    a.scopes.push(Scope::Commit { sha: "HEAD".into() });
    a.pick_commit("a".repeat(40));
    a.pick_commit("b".repeat(40));

    let commits = a
        .scopes
        .iter()
        .filter(|s| matches!(s, Scope::Commit { .. }))
        .count();
    assert_eq!(commits, 1, "one commit tab, retargeted");
    assert_eq!(
        a.scope,
        Scope::Commit {
            sha: "b".repeat(40)
        }
    );
}

#[test]
fn a_repository_opened_without_a_commit_tab_still_gains_one() {
    let mut a = app();
    assert!(!a.scopes.iter().any(|s| matches!(s, Scope::Commit { .. })));
    a.pick_commit("a".repeat(40));
    assert!(
        a.scopes.contains(&a.scope),
        "the tab exists to come back to"
    );
}

#[test]
fn the_history_lands_on_the_commit_already_under_review() {
    use crate::service::Response;
    let mut a = app();
    a.scope = Scope::Commit {
        sha: "b".repeat(40),
    };
    a.open_history();
    a.apply(Response::Log(Ok(vec![
        commit('c', "newest", "x"),
        commit('b', "current", "x"),
        commit('a', "oldest", "x"),
    ])));
    assert_eq!(a.sel, 1, "the cursor starts where the dot is");
}

// --- modals ---

#[test]
fn a_modal_owns_the_keyboard_while_it_is_up() {
    let mut a = app();
    press(&mut a, Key::Char('/'));
    let before = a.cursor;
    typed(&mut a, "jjj");
    assert_eq!(a.cursor, before, "the letters were the query");
    assert_eq!(a.query, "jjj");
}

#[test]
fn tab_walks_the_finder_scopes_and_comes_back_round() {
    let mut a = app();
    press(&mut a, Key::Char('/'));
    for _ in 0..FinderTab::ALL.len() {
        press(&mut a, Key::Tab);
    }
    assert_eq!(a.finder_tab, FinderTab::Files);
}

#[test]
fn the_finder_lists_the_files_and_jumping_opens_one() {
    let mut a = app();
    press(&mut a, Key::Char('/'));
    typed(&mut a, "b.rs");
    assert!(!a.hits().is_empty());
    press(&mut a, Key::Enter);
    assert_eq!(a.path(), "src/b.rs");
    assert!(a.modal.is_none());
}

#[test]
fn the_palette_runs_what_it_offers() {
    let mut a = app();
    assert!(!a.blame_on);
    press(&mut a, Key::Char(':'));
    typed(&mut a, "blame");
    press(&mut a, Key::Enter);
    assert!(a.blame_on, "the command actually ran");
    assert!(a.modal.is_none());
}

#[test]
fn escape_closes_whatever_is_up() {
    for k in ['/', ':'] {
        let mut a = app();
        press(&mut a, Key::Char(k));
        assert!(a.modal.is_some(), "{k} should open something");
        press(&mut a, Key::Esc);
        assert!(a.modal.is_none(), "{k} should close on esc");
    }
    // and the ones that moved under the leader when the plain keys
    // became motions
    for k in ['a', 'g', '?'] {
        let mut a = app();
        leader(&mut a, k);
        assert!(a.modal.is_some(), "leader {k} should open something");
        press(&mut a, Key::Esc);
        assert!(a.modal.is_none(), "leader {k} should close on esc");
    }
}

#[test]
fn every_palette_entry_is_something_run_command_handles() {
    // the guard against a command that looks available and does nothing
    for (label, _) in COMMANDS {
        let mut a = app();
        assert!(
            a.run_command(label),
            "{label} is offered in the palette but nothing runs it"
        );
    }
}

#[test]
fn change_motions_do_not_panic_on_an_empty_diff() {
    let mut a = app();
    a.rows.insert(a.path().to_string(), Vec::new());
    a.run(Action::ChangeNext, 1);
    a.run(Action::ChangePrev, 1);
}

#[test]
fn screen_middle_does_not_underflow_when_scroll_exceeds_rows() {
    let mut a = app();
    a.diff_scroll = 500;
    a.view_height = 20;
    a.run(Action::ScreenMiddle, 1);
    assert!(a.cursor < a.diff_rows().len());
}

#[test]
fn typing_many_digits_saturates_the_count_without_overflowing() {
    let mut a = app();
    for _ in 0..25 {
        press(&mut a, Key::Char('9'));
    }
    assert!(a.count.is_some());
    press(&mut a, Key::Char('j'));
    for _ in 0..25 {
        press(&mut a, Key::Char('9'));
    }
    press(&mut a, Key::Char('l'));
}

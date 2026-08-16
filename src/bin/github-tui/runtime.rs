//! Process setup and command dispatch for `github-tui`.
//!
//! The terminal adapter itself lives in `program`, leaving this module to own
//! only the resources and exit paths of one process run.

use std::io;

use github_tui::tui::run::{Terminal_, run as run_tui};

use github_tui::github::cli::{self, Command, RenderArgs, RenderKind};
use github_tui::github::{gh, snapshot};
use github_tui::shared::config;

use super::program::GithubTui;

pub(super) fn run() -> io::Result<()> {
    let command = match cli::parse_args(std::env::args_os().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            // No terminal or log exists yet, so this skips no cleanup.
            eprintln!("github-tui: {error}");
            std::process::exit(2);
        }
    };

    let args = match command {
        Command::Run(args) => args,
        Command::Render(args) => {
            render(&args);
            return Ok(());
        }
        Command::Help(text) | Command::Version(text) => {
            println!("{text}");
            return Ok(());
        }
    };

    // Deliberately after the headless modes: a snapshot has to render the same
    // frame on any machine, so it stays on the default theme regardless of
    // what this user picked.
    config::apply_theme();

    // There is nothing to fall back to, so not being able to reach GitHub is
    // the end of the run rather than the start of a pretend one. Saying which
    // of the two it was, because "not installed" and "not signed in" are fixed
    // differently.
    if !gh::available() {
        eprintln!("gh-tui: gh is unavailable or not signed in.");
        eprintln!("        install it from https://cli.github.com, then `gh auth login`.");
        return Ok(());
    }

    // Before the terminal is taken: a file that cannot be opened is worth
    // saying so about while there is still a screen to say it on.
    if let Some(path) = args.log {
        if let Err(error) = github_tui::shared::log::to(&path, "github-tui", "--svg-live") {
            eprintln!("github-tui: cannot write to {}: {error}", path.display());
            return Ok(());
        }
        github_tui::shared::log::say(format_args!("source gh"));
    }

    // Not `?`: "it does not start" is the report a log is most wanted for,
    // and propagating here would close the file having written the header and
    // nothing else.
    let mut term = match Terminal_::enter(args.mouse) {
        Ok(term) => term,
        Err(error) => {
            github_tui::shared::log::say(format_args!("could not take the terminal: {error}"));
            return Err(error);
        }
    };
    let result = run_tui(&mut term, &mut GithubTui::new());
    // The guard restores the terminal even if the loop returns an error.
    drop(term);

    if let Err(error) = &result {
        github_tui::shared::log::say(format_args!("ended with: {error}"));
        eprintln!("github-tui: {error}");
    }
    github_tui::shared::log::finish();
    result
}

fn render(args: &RenderArgs) {
    // The live form is what the last line of a session log names. The others
    // use the fixture, which is what makes them reproducible on any machine.
    match args.kind {
        RenderKind::Snapshot => {
            snapshot::run(&args.keys, args.width, args.height, args.ticks);
        }
        RenderKind::Svg => snapshot::svg(&args.keys, args.width, args.height, args.ticks),
        RenderKind::SvgLoading => {
            snapshot::svg_loading(&args.keys, args.width, args.height, args.ticks as u64);
        }
        RenderKind::SvgLive => {
            snapshot::svg_live(&args.keys, args.width, args.height, args.ticks);
        }
    }
}

//! Process setup, repository discovery and headless rendering for `diffline`.
//!
//! The terminal adapter itself lives in `program`, leaving this module to own
//! only the resources and exit paths of one process run.

use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use tui_kit::run::{Terminal_, run as run_tui};

use diffline_app::app::App;
use diffline_app::cli::{self, Command, RenderArgs, RunArgs};
use diffline_app::model::Scope;
use diffline_app::view as ui;

use super::program::Diffline;

pub(super) fn run() -> io::Result<()> {
    let command = match cli::parse_args(std::env::args_os().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            // No terminal or log exists yet, so this skips no cleanup.
            eprintln!("diffline: {error}");
            std::process::exit(2);
        }
    };

    match command {
        Command::Run(args) => interactive(args),
        Command::Render(args) => {
            render(&args);
            Ok(())
        }
        Command::Help(text) | Command::Version(text) => {
            println!("{text}");
            Ok(())
        }
    }
}

fn interactive(args: RunArgs) -> io::Result<()> {
    let Some(mut app) = open_repository(&args.repository) else {
        return Ok(());
    };

    // Opened after the repository is known, so the first line of the log says
    // which one this was — and before the terminal is taken, so a failure to
    // open the file is still something that can be printed.
    if let Some(path) = args.log {
        if let Err(error) = line_shared::log::to(&path, "diffline", "--svg") {
            eprintln!("diffline: cannot write to {}: {error}", path.display());
            return Ok(());
        }
        line_shared::log::say(format_args!("repo {}", app.repo));
    }

    // A watcher failure should not make the existing manual refresh unusable.
    // Started before taking the terminal so its concrete backend error has a
    // normal stderr to be printed on.
    let watch = match diffline_app::watch::Watch::start(Path::new(&app.repo)) {
        Ok(watch) => Some(watch),
        Err(error) => {
            eprintln!("diffline: cannot watch {}: {error}", app.repo);
            None
        }
    };

    // Not `?`: "it does not start" is the report a log is most wanted for,
    // and propagating here would close the file having written the header
    // and nothing else.
    let mut term = match Terminal_::enter(args.mouse) {
        Ok(term) => term,
        Err(error) => {
            line_shared::log::say(format_args!("could not take the terminal: {error}"));
            return Err(error);
        }
    };
    let result = run_tui(&mut term, &mut Diffline::new(&mut app, watch));
    // The guard gives the terminal back even if the loop returned an error.
    drop(term);

    if let Err(error) = &result {
        line_shared::log::say(format_args!("ended with: {error}"));
        eprintln!("diffline: {error}");
    }
    line_shared::log::finish();
    result
}

fn render(args: &RenderArgs) {
    let Some(mut app) = open_repository(&args.repository) else {
        return;
    };
    headless(&mut app, &args.keys, args.width, args.height);
}

fn open_repository(repository: &Path) -> Option<App> {
    // The repository is where you are unless you say otherwise, because that
    // is almost always the one whose diff you meant.
    let repository = std::fs::canonicalize(repository).unwrap_or_else(|_| repository.into());
    let repository = repository.to_string_lossy().into_owned();

    // Which backend owns this directory, asked once. Nothing below here knows
    // it was git rather than something else.
    let Some(vcs) = diffline_app::vcs::of(&repository) else {
        eprintln!("diffline: {repository} is not a repository anything here can read");
        return None;
    };

    line_shared::config::apply_theme();

    let base = vcs.base_branch(&repository);
    let head = vcs
        .head_branch(&repository)
        .unwrap_or_else(|| "HEAD".into());

    // Three scopes, widening: what is not committed, what this branch has
    // that base does not, and the last commit on its own.
    let mut scopes = vec![Scope::WorkingTree];
    if head != base {
        scopes.push(Scope::Branch { base });
    }
    scopes.push(Scope::Commit { sha: "HEAD".into() });

    // Open on whichever of the first two actually has something in it: a
    // clean tree should not greet you with an empty pane.
    let opening = scopes
        .iter()
        .find(|scope| {
            vcs.changed_files(&repository, scope)
                .map(|files| !files.is_empty())
                .unwrap_or(false)
        })
        .cloned()
        .unwrap_or(Scope::WorkingTree);

    // The worker is started here, which is the only place that should decide
    // there is one: a snapshot wants none, and a test wants to hand in its
    // own.
    Some(App::new(
        repository,
        opening,
        scopes,
        Some(Box::new(diffline_app::service::Service::spawn())),
    ))
}

/// Draws one frame into an off-screen terminal and prints it as SVG.
///
/// `settle` waits for the worker rather than guessing at a delay, so the frame
/// is of the finished state and not of whatever had arrived by then.
fn headless(app: &mut App, keys: &str, width: u16, height: u16) {
    use ratatui::backend::TestBackend;

    settle(app);
    for key in line_shared::key::parse_keys(keys) {
        app.on_key(key);
        settle(app);
    }

    // `TestBackend`'s error type is `Infallible` from ratatui 0.30 on: an
    // off-screen buffer has nowhere to fail. The irrefutable `let Ok(…)` is
    // how that is said without an `unwrap` and without a panic.
    let Ok(mut term) = ratatui::Terminal::new(TestBackend::new(width, height));
    let Ok(_) = term.draw(|frame| ui::draw(frame, app));
    print!(
        "{}",
        tui_kit::svg::render(term.backend().buffer(), width, height)
    );
}

fn settle(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        app.ensure();
        while let Some(response) = app.poll() {
            app.apply(response);
        }
        if !app.waiting() || Instant::now() > deadline {
            app.ensure();
            if !app.waiting() || Instant::now() > deadline {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

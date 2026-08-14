//! Invoking the `gh` CLI and translating its JSON into the app's model.
//!
//! Everything here blocks: it always runs on the service thread, never on the
//! render loop.

use std::process::Command;

use serde_json::Value;

use crate::data::{
    Account, Comment, Detail, FileChange, Hunk, IssueDetail, Item, Job, Label, PrDetail, RawLog,
    Repo, Review, ReviewState, RunDetail, Status, Step, TreeEntry,
};

pub use crate::error::{Error, Result as Res};

/// The leading arguments identify the call in error messages; the rest is
/// usually a long query that adds nothing.
fn label(args: &[&str]) -> String {
    args.iter()
        .take_while(|a| !a.starts_with('-'))
        .take(3)
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Runs `gh` and returns its stdout.
fn run(args: &[&str]) -> Res<String> {
    let out = Command::new("gh")
        .args(args)
        .output()
        .map_err(Error::Spawn)?;
    if !out.status.success() {
        return Err(Error::Command {
            args: label(args),
            status: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn json(args: &[&str]) -> Res<Value> {
    let raw = run(args)?;
    serde_json::from_str(&raw).map_err(|e| Error::Json {
        args: label(args),
        source: e,
    })
}

/// Is there a usable `gh` with a signed-in session?
pub fn available() -> bool {
    Command::new("gh")
        .args(["auth", "status"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ------------------------------------------------------------------ utilidades

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}

fn n(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(serde_json::Value::as_i64).unwrap_or(0)
}

fn arr(v: &Value, key: &str) -> Vec<Value> {
    v.get(key)
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default()
}

/// ISO-8601 date → "3h ago", in the design's compact format.
fn ago(iso: &str) -> String {
    let Some(then) = parse_iso(iso) else {
        return String::new();
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let secs = (now - then).max(0);
    match secs {
        s if s < 60 => "just now".to_string(),
        s if s < 3600 => format!("{}m ago", s / 60),
        s if s < 86_400 => format!("{}h ago", s / 3600),
        s if s < 604_800 => format!("{}d ago", s / 86_400),
        s => format!("{}w ago", s / 604_800),
    }
}

/// Epoch seconds of a UTC ISO-8601 date (`2026-08-13T21:45:34Z`).
fn parse_iso(iso: &str) -> Option<i64> {
    let b = iso.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let num = |a: usize, z: usize| iso[a..z].parse::<i64>().ok();
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, se) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);

    // days from the civil era (Howard Hinnant's algorithm)
    let yy = if mo <= 2 { y - 1 } else { y };
    let era = if yy >= 0 { yy } else { yy - 399 } / 400;
    let yoe = yy - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3600 + mi * 60 + se)
}

/// Elapsed time between two ISO stamps, in the design's "2m 14s" format.
fn duration(start: &str, end: &str) -> String {
    // anything unfinished arrives as 0001-01-01T00:00:00Z
    if start.starts_with("0001") || end.starts_with("0001") || end.is_empty() {
        return "—".to_string();
    }
    let (Some(a), Some(b)) = (parse_iso(start), parse_iso(end)) else {
        return "—".to_string();
    };
    let secs = (b - a).max(0);
    if secs >= 60 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

/// The state of an Actions check/job in the design's vocabulary.
///
/// A run that has not completed reports no conclusion yet, so the status field
/// is what decides; once completed, the conclusion takes over.
fn conclusion(status: &str, conclusion: &str) -> Status {
    if !status.eq_ignore_ascii_case("completed") {
        return match Status::parse(status) {
            Status::Running => Status::Running,
            _ => Status::Pending,
        };
    }
    match Status::parse(conclusion) {
        Status::Unknown => Status::Pending,
        settled => settled,
    }
}

/// A GitHub label colour (hex without `#`) as plain RGB.
fn label_rgb(hex: &str) -> (u8, u8, u8) {
    let v = u32::from_str_radix(hex, 16).unwrap_or(0x0088_8888);
    (
        ((v >> 16) & 255) as u8,
        ((v >> 8) & 255) as u8,
        (v & 255) as u8,
    )
}

// -------------------------------------------------------------------- cuentas

/// The signed-in user and their organisations, as switchable accounts.
pub fn accounts() -> Res<Vec<Account>> {
    let me = json(&["api", "user"])?;
    let login = s(&me, "login");
    if login.is_empty() {
        return Err(Error::Field {
            args: "api user".into(),
            field: "login",
        });
    }
    let host = "github.com";

    let mut out = vec![Account {
        login: login.clone(),
        kind: "(personal)".into(),
        sub: format!("{host} · {}", s(&me, "name")),
        repos: Vec::new(),
    }];

    if let Ok(orgs) = json(&["api", "user/orgs"]) {
        for o in orgs.as_array().cloned().unwrap_or_default() {
            out.push(Account {
                login: s(&o, "login"),
                kind: "(org)".into(),
                sub: format!("{host} · member"),
                repos: Vec::new(),
            });
        }
    }
    Ok(out)
}

const REPO_QUERY: &str = r#"query($login:String!){
  repositoryOwner(login:$login){
    repositories(first:60, orderBy:{field:PUSHED_AT,direction:DESC}, ownerAffiliations:OWNER){
      nodes{
        name isPrivate stargazerCount
        primaryLanguage{ name }
        issues(states:OPEN){ totalCount }
        pullRequests(states:OPEN){ totalCount }
        object(expression:"HEAD:.github/workflows"){ ... on Tree { entries { name } } }
      }
    }
  }
}"#;

/// An owner's repos with their open issue and PR counts, in a single GraphQL
/// query: exactly what the sidebar needs.
pub fn repos(login: &str) -> Res<Vec<Repo>> {
    let v = json(&[
        "api",
        "graphql",
        "-f",
        &format!("login={login}"),
        "-f",
        &format!("query={REPO_QUERY}"),
    ])?;
    let nodes = v
        .pointer("/data/repositoryOwner/repositories/nodes")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(nodes
        .iter()
        .map(|r| Repo {
            name: s(r, "name"),
            private: r
                .get("isPrivate")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            lang: r
                .pointer("/primaryLanguage/name")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            issues: r
                .pointer("/issues/totalCount")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as u32,
            prs: r
                .pointer("/pullRequests/totalCount")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as u32,
            star: r
                .get("stargazerCount")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
                .to_string(),
            // a tree is only there when the directory is
            has_workflows: r.pointer("/object/entries").is_some(),
        })
        .collect())
}

// --------------------------------------------------------------------- listas

fn labels_of(v: &Value) -> Vec<Label> {
    arr(v, "labels")
        .iter()
        .map(|l| Label::new(&s(l, "name"), label_rgb(&s(l, "color"))))
        .collect()
}

pub fn issues(repo: &str) -> Res<Vec<Item>> {
    let v = json(&[
        "issue",
        "list",
        "-R",
        repo,
        "--state",
        "all",
        "--limit",
        "40",
        "--json",
        "number,title,state,author,createdAt,comments,labels",
    ])?;

    Ok(v.as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|i| {
            let mut it = Item::issue();
            it.num = n(i, "number");
            it.title = s(i, "title");
            it.state = Status::parse(&s(i, "state"));
            it.author = i
                .pointer("/author/login")
                .and_then(|x| x.as_str())
                .unwrap_or("ghost")
                .to_string();
            it.when = ago(&s(i, "createdAt"));
            it.labels = labels_of(i);
            it.detail = Detail::Issue(IssueDetail {
                comments: arr(i, "comments").len() as u32,
                comment_list: Vec::new(),
            });
            it
        })
        .collect())
}

/// Id of the workflow run a PR's checks belong to. It is embedded in the check
/// URL (`…/actions/runs/<id>/job/<id>`) and is what lets us show its jobs and
/// its logs.
fn rollup_run_id(v: &Value) -> i64 {
    for c in arr(v, "statusCheckRollup") {
        let url = s(&c, "detailsUrl");
        if let Some(rest) = url.split("/actions/runs/").nth(1) {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            if let Ok(id) = digits.parse::<i64>() {
                return id;
            }
        }
    }
    0
}

/// Reduces a PR's check rollup to a single state.
fn rollup(v: &Value) -> Status {
    let checks = arr(v, "statusCheckRollup");
    if checks.is_empty() {
        return Status::Pending;
    }
    let states: Vec<Status> = checks
        .iter()
        .map(|c| {
            // CheckRun carries status/conclusion; StatusContext only has state
            if c.get("status").is_some() {
                conclusion(&s(c, "status"), &s(c, "conclusion"))
            } else {
                conclusion("completed", &s(c, "state"))
            }
        })
        .collect();

    if states.contains(&Status::Failure) {
        Status::Failure
    } else if states.contains(&Status::Running) {
        Status::Running
    } else if states.contains(&Status::Pending) {
        Status::Pending
    } else {
        Status::Success
    }
}

pub fn prs(repo: &str) -> Res<Vec<Item>> {
    let v = json(&[
        "pr",
        "list",
        "-R",
        repo,
        "--state",
        "all",
        "--limit",
        "40",
        "--json",
        "number,title,state,author,createdAt,headRefName,additions,deletions,changedFiles,isDraft,labels,statusCheckRollup",
    ])?;

    Ok(v.as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|p| {
            let mut it = Item::pr();
            it.num = n(p, "number");
            it.title = s(p, "title");
            let draft = p
                .get("isDraft")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let state = Status::parse(&s(p, "state"));
            it.state = if draft && state == Status::Open {
                Status::Draft
            } else {
                state
            };
            it.author = p
                .pointer("/author/login")
                .and_then(|x| x.as_str())
                .unwrap_or("ghost")
                .to_string();
            it.when = ago(&s(p, "createdAt"));
            it.id = rollup_run_id(p);
            it.labels = labels_of(p);
            it.detail = Detail::Pr(Box::new(PrDetail {
                checks: rollup(p),
                add: format!("+{}", n(p, "additions")),
                del: format!("-{}", n(p, "deletions")),
                files: n(p, "changedFiles") as u32,
                branch: s(p, "headRefName"),
                workflow: arr(p, "statusCheckRollup")
                    .iter()
                    .map(|c| s(c, "workflowName"))
                    .find(|w| !w.is_empty())
                    .unwrap_or_default(),
                ..PrDetail::default()
            }));
            it
        })
        .collect())
}

pub fn runs(repo: &str) -> Res<Vec<Item>> {
    let v = json(&[
        "run",
        "list",
        "-R",
        repo,
        "--limit",
        "40",
        "--json",
        "databaseId,number,displayTitle,status,conclusion,event,createdAt,updatedAt,workflowName,headBranch",
    ])?;

    Ok(v.as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|r| {
            let mut it = Item::run();
            // databaseId is for the API; the number is what GitHub displays
            it.id = n(r, "databaseId");
            it.num = n(r, "number");
            it.title = format!("{} · {}", s(r, "workflowName"), s(r, "headBranch"));
            it.state = conclusion(&s(r, "status"), &s(r, "conclusion"));
            it.author = s(r, "displayTitle");
            it.when = ago(&s(r, "createdAt"));
            it.detail = Detail::Run(RunDetail {
                event: s(r, "event"),
                workflow: s(r, "workflowName"),
                dur: duration(&s(r, "createdAt"), &s(r, "updatedAt")),
            });
            it
        })
        .collect())
}

// -------------------------------------------------------------------- detalle

/// A PR's body, files and reviews; requested when the detail is opened.
pub fn pr_detail(repo: &str, num: i64) -> Res<(String, Vec<FileChange>, Vec<Review>)> {
    let v = json(&[
        "pr",
        "view",
        &num.to_string(),
        "-R",
        repo,
        "--json",
        "body,files,reviews",
    ])?;

    let files = arr(&v, "files")
        .iter()
        .map(|f| FileChange {
            path: s(f, "path"),
            add: format!("+{}", n(f, "additions")),
            del: format!("-{}", n(f, "deletions")),
            hunks: Vec::new(), // llegan aparte, con `gh pr diff`
        })
        .collect();

    let reviews = arr(&v, "reviews")
        .iter()
        .map(|r| Review {
            author: r
                .pointer("/author/login")
                .and_then(|x| x.as_str())
                .unwrap_or("ghost")
                .to_string(),
            state: ReviewState::parse(&s(r, "state")),
        })
        .collect();

    Ok((s(&v, "body"), files, reviews))
}

/// An issue's body and comments.
pub fn issue_detail(repo: &str, num: i64) -> Res<(String, Vec<Comment>)> {
    let v = json(&[
        "issue",
        "view",
        &num.to_string(),
        "-R",
        repo,
        "--json",
        "body,comments",
    ])?;

    let comments = arr(&v, "comments")
        .iter()
        .map(|c| Comment {
            author: c
                .pointer("/author/login")
                .and_then(|x| x.as_str())
                .unwrap_or("ghost")
                .to_string(),
            when: ago(&s(c, "createdAt")),
            body: s(c, "body"),
        })
        .collect();

    Ok((s(&v, "body"), comments))
}

/// A workflow run's jobs and steps: this feeds the log view's tree.
pub fn run_jobs(repo: &str, run_id: i64) -> Res<Vec<Job>> {
    let v = json(&[
        "run",
        "view",
        &run_id.to_string(),
        "-R",
        repo,
        "--json",
        "jobs",
    ])?;

    Ok(arr(&v, "jobs")
        .iter()
        .map(|j| Job {
            name: s(j, "name"),
            status: conclusion(&s(j, "status"), &s(j, "conclusion")),
            dur: duration(&s(j, "startedAt"), &s(j, "completedAt")),
            steps: arr(j, "steps")
                .iter()
                .map(|st| Step {
                    name: s(st, "name"),
                    status: conclusion(&s(st, "status"), &s(st, "conclusion")),
                    dur: duration(&s(st, "startedAt"), &s(st, "completedAt")),
                })
                .collect(),
        })
        .collect())
}

/// Strips the ANSI escapes and control characters that Actions logs carry:
/// here the colour comes from `log_kind`, not from the runner.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        // the `gh run view --log` dump writes ESC as the two literal characters
        // `^[`, not as 0x1b
        if c == '^' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        if c == '\u{1b}' {
            // CSI / OSC: discard up to the sequence's final letter
            if chars.peek() == Some(&'[') {
                chars.next();
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                chars.next();
            }
            continue;
        }
        if c == '\r' || (c.is_control() && c != '\t') {
            continue;
        }
        out.push(c);
    }
    out
}

/// Classifies a log line for colouring, the way the Actions web UI does.
fn log_kind(line: &str) -> &'static str {
    let l = line.trim_start();
    if l.starts_with("##[group]") || l.starts_with("##[endgroup]") {
        "group"
    } else if l.starts_with("##[error]")
        || l.contains("panicked at")
        || l.contains("FAILED")
        || l.starts_with("error:")
        || l.starts_with("error[")
    {
        "red"
    } else if l.starts_with("##[warning]") || l.starts_with("warning:") {
        "yellow"
    } else if l.contains("... ok") || l.starts_with("test result: ok") {
        "green"
    } else if l.starts_with("##[command]") || l.starts_with('+') {
        "dim"
    } else {
        "fg"
    }
}

/// A run's full log. `gh` returns it as `job⇥step⇥timestamp message`, which is
/// exactly the granularity the log view's tree needs.
pub fn run_log(repo: &str, run_id: i64, finished: bool) -> Res<Vec<RawLog>> {
    // `--log` only exists for finished runs; live ones would need `--log-failed`
    // or the partial `view --job` dump, so we take whatever is available.
    let id = run_id.to_string();
    let raw = if finished {
        run(&["run", "view", &id, "-R", repo, "--log"])?
    } else {
        run(&["run", "view", &id, "-R", repo, "--log"]).unwrap_or_default()
    };

    Ok(raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let l = l.trim_start_matches('\u{feff}');
            let mut parts = l.splitn(3, '\t');
            let job = parts.next().unwrap_or("").to_string();
            let step = parts.next().unwrap_or("").to_string();
            let rest = parts.next().unwrap_or("");
            // the remainder starts with an ISO timestamp
            let (time, text) = match rest.find(' ') {
                Some(i) if rest.len() > 19 => {
                    let stamp = &rest[..i];
                    let hhmmss = stamp.get(11..19).unwrap_or("").to_string();
                    (hhmmss, strip_ansi(&rest[i + 1..]))
                }
                _ => (String::new(), strip_ansi(rest)),
            };
            let kind = log_kind(&text);
            RawLog {
                job,
                step,
                time,
                text,
                kind,
            }
        })
        .collect())
}

/// A PR's full diff, split per file.
///
/// `gh pr diff` returns a standard unified diff; it is split into files by the
/// `diff --git` headers and into hunks by the `@@` lines.
pub fn pr_diff(repo: &str, num: i64) -> Res<Vec<(String, Vec<Hunk>)>> {
    let raw = run(&["pr", "diff", &num.to_string(), "-R", repo])?;
    Ok(parse_unified(&raw))
}

fn parse_unified(raw: &str) -> Vec<(String, Vec<Hunk>)> {
    let mut files: Vec<(String, Vec<Hunk>)> = Vec::new();
    let mut hunk: Option<Hunk> = None;

    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            if let Some(h) = hunk.take()
                && let Some(f) = files.last_mut()
            {
                f.1.push(h);
            }
            // `a/path b/path`; the second one is the new file's path
            let path = rest.split(" b/").nth(1).unwrap_or(rest).trim().to_string();
            files.push((path, Vec::new()));
            continue;
        }
        if files.is_empty() {
            continue;
        }
        if line.starts_with("@@") {
            if let (Some(h), Some(f)) = (hunk.take(), files.last_mut()) {
                f.1.push(h);
            }
            hunk = Some(Hunk {
                hdr: line.to_string(),
                lines: Vec::new(),
            });
            continue;
        }
        let Some(h) = hunk.as_mut() else { continue };
        // inside a hunk the first character is the sign
        let mut chars = line.chars();
        match chars.next() {
            Some(sign @ ('+' | '-' | ' ')) => h.lines.push((sign, chars.as_str().to_string())),
            None => h.lines.push((' ', String::new())),
            // `\ No newline at end of file` and friends are ignored
            _ => {}
        }
    }
    if let Some(h) = hunk
        && let Some(f) = files.last_mut()
    {
        f.1.push(h);
    }
    files
}

// --------------------------------------------------------------------- search

/// Issues or pull requests across the owner's repositories.
///
/// An empty query is fine here — GitHub allows qualifier-only searches for
/// issues — and returns the most recently updated.
pub fn search_issues(owner: &str, query: &str, prs: bool) -> Res<Vec<SearchHit>> {
    let kind = if prs { "prs" } else { "issues" };
    let mut args = vec![
        "search",
        kind,
        "--owner",
        owner,
        "--limit",
        "40",
        "--json",
        "number,title,repository,state,updatedAt",
    ];
    let trimmed = query.trim();
    if !trimmed.is_empty() {
        args.push(trimmed);
    }
    let v = json(&args)?;

    Ok(v.as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|h| SearchHit {
            title: s(h, "title"),
            repo: h
                .pointer("/repository/nameWithOwner")
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string(),
            num: n(h, "number"),
            state: Status::parse(&s(h, "state")),
            when: ago(&s(h, "updatedAt")),
            sha: String::new(),
        })
        .collect())
}

/// Commits across the owner's repositories.
///
/// GitHub rejects a commit search made of qualifiers alone, so this is only
/// called once there is something to search for.
pub fn search_commits(owner: &str, query: &str) -> Res<Vec<SearchHit>> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let v = json(&[
        "search",
        "commits",
        "--owner",
        owner,
        "--limit",
        "40",
        "--json",
        "sha,commit,repository",
        trimmed,
    ])?;

    Ok(v.as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|h| {
            let sha = s(h, "sha");
            let message = h
                .pointer("/commit/message")
                .and_then(|x| x.as_str())
                .unwrap_or_default();
            SearchHit {
                // a commit message is a subject plus a body; only the subject
                // belongs on one row
                title: message.lines().next().unwrap_or_default().to_string(),
                // commit search names the repository `fullName`, unlike the
                // issue search next door
                repo: h
                    .pointer("/repository/fullName")
                    .or_else(|| h.pointer("/repository/nameWithOwner"))
                    .and_then(|x| x.as_str())
                    .unwrap_or_default()
                    .to_string(),
                num: 0,
                state: Status::Unknown,
                when: h
                    .pointer("/commit/author/date")
                    .and_then(|x| x.as_str())
                    .map(ago)
                    .unwrap_or_default(),
                sha: sha.chars().take(7).collect(),
            }
        })
        .collect())
}

/// A row as the search API returns it, before the finder shapes it.
pub struct SearchHit {
    pub title: String,
    pub repo: String,
    pub num: i64,
    pub state: Status,
    pub when: String,
    pub sha: String,
}

// -------------------------------------------------------------------- acciones

pub fn merge(repo: &str, num: i64, method: &str) -> Res<()> {
    let flag = match method {
        "squash" => "--squash",
        "rebase" => "--rebase",
        _ => "--merge",
    };
    run(&["pr", "merge", &num.to_string(), "-R", repo, flag]).map(|_| ())
}

pub fn close(repo: &str, num: i64) -> Res<()> {
    run(&["pr", "close", &num.to_string(), "-R", repo]).map(|_| ())
}

pub fn reopen(repo: &str, num: i64) -> Res<()> {
    run(&["pr", "reopen", &num.to_string(), "-R", repo]).map(|_| ())
}

/// `gh` exposes no standalone branch deletion, so this goes through the API.
pub fn delete_branch(repo: &str, branch: &str) -> Res<()> {
    run(&[
        "api",
        "-X",
        "DELETE",
        &format!("repos/{repo}/git/refs/heads/{branch}"),
    ])
    .map(|_| ())
}

// ------------------------------------------------------- todos los repos
//
// The "all repositories" pseudo-repo. Issues and pull requests come from one
// GraphQL search each — the REST search would not carry the diff counts a row
// needs, and one query beats one per repository by a wide margin.

/// Issues and pull requests share a search; only the fragment differs.
const ALL_QUERY: &str = r#"query($q:String!){
  search(query:$q, type:ISSUE, first:60){
    nodes{
      ... on Issue {
        number title state createdAt
        author{ login }
        repository{ nameWithOwner }
        comments{ totalCount }
        labels(first:10){ nodes{ name color } }
      }
      ... on PullRequest {
        number title state isDraft createdAt
        additions deletions changedFiles headRefName
        author{ login }
        repository{ nameWithOwner }
        labels(first:10){ nodes{ name color } }
        commits(last:1){ nodes{ commit{ statusCheckRollup{
          state
          contexts(first:10){ nodes{ __typename
            ... on CheckRun{ detailsUrl checkSuite{ workflowRun{ workflow{ name } } } }
          } }
        } } } }
      }
    }
  }
}"#;

fn search_all(owner: &str, kind: &str) -> Res<Vec<Value>> {
    // `sort:updated-desc` so the newest work is at the top, which is the only
    // ordering that makes a list spanning sixty repositories worth reading.
    let q = format!("owner:{owner} is:{kind} sort:updated-desc");
    let v = json(&[
        "api",
        "graphql",
        "-f",
        &format!("q={q}"),
        "-f",
        &format!("query={ALL_QUERY}"),
    ])?;
    Ok(v.pointer("/data/search/nodes")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default())
}

/// GraphQL nests labels one level deeper than the CLI does.
fn gql_labels(v: &Value) -> Vec<Label> {
    v.pointer("/labels/nodes")
        .and_then(|x| x.as_array())
        .map(|ls| {
            ls.iter()
                .map(|l| Label::new(&s(l, "name"), label_rgb(&s(l, "color"))))
                .collect()
        })
        .unwrap_or_default()
}

fn gql_author(v: &Value) -> String {
    v.pointer("/author/login")
        .and_then(|x| x.as_str())
        .unwrap_or("ghost")
        .to_string()
}

fn gql_repo(v: &Value) -> String {
    v.pointer("/repository/nameWithOwner")
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string()
}

/// Open issues across every repository the owner has.
pub fn all_issues(owner: &str) -> Res<Vec<Item>> {
    Ok(search_all(owner, "issue")?
        .iter()
        .map(|i| {
            let mut it = Item::issue();
            it.num = n(i, "number");
            it.repo = gql_repo(i);
            it.title = s(i, "title");
            it.state = Status::parse(&s(i, "state"));
            it.author = gql_author(i);
            it.when = ago(&s(i, "createdAt"));
            it.labels = gql_labels(i);
            it.detail = Detail::Issue(IssueDetail {
                comments: i
                    .pointer("/comments/totalCount")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0) as u32,
                comment_list: Vec::new(),
            });
            it
        })
        .collect())
}

/// The single commit's check rollup, as `(state, run id, workflow name)`.
fn gql_rollup(p: &Value) -> (Status, i64, String) {
    let Some(rollup) = p.pointer("/commits/nodes/0/commit/statusCheckRollup") else {
        return (Status::Pending, 0, String::new());
    };
    let state = conclusion("completed", &s(rollup, "state"));
    let contexts = rollup
        .pointer("/contexts/nodes")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();

    // the run id rides in the check's URL, exactly as in the REST rollup
    let id = contexts
        .iter()
        .find_map(|c| run_id_in(&s(c, "detailsUrl")))
        .unwrap_or(0);
    let workflow = contexts
        .iter()
        .find_map(|c| {
            let w = c
                .pointer("/checkSuite/workflowRun/workflow/name")
                .and_then(|x| x.as_str())
                .unwrap_or_default();
            (!w.is_empty()).then(|| w.to_string())
        })
        .unwrap_or_default();
    (state, id, workflow)
}

/// Digs the workflow run's id out of a check URL
/// (`…/actions/runs/<id>/job/<id>`).
fn run_id_in(url: &str) -> Option<i64> {
    let rest = url.split("/actions/runs/").nth(1)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Pull requests across every repository the owner has.
pub fn all_prs(owner: &str) -> Res<Vec<Item>> {
    Ok(search_all(owner, "pr")?
        .iter()
        .map(|p| {
            let mut it = Item::pr();
            it.num = n(p, "number");
            it.repo = gql_repo(p);
            it.title = s(p, "title");
            let draft = p
                .get("isDraft")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let state = Status::parse(&s(p, "state"));
            it.state = if draft && state == Status::Open {
                Status::Draft
            } else {
                state
            };
            it.author = gql_author(p);
            it.when = ago(&s(p, "createdAt"));
            it.labels = gql_labels(p);

            let (checks, id, workflow) = gql_rollup(p);
            it.id = id;
            it.detail = Detail::Pr(Box::new(PrDetail {
                checks,
                add: format!("+{}", n(p, "additions")),
                del: format!("-{}", n(p, "deletions")),
                files: n(p, "changedFiles") as u32,
                branch: s(p, "headRefName"),
                workflow,
                ..PrDetail::default()
            }));
            it
        })
        .collect())
}

/// How many `gh run list` calls are in flight at once.
///
/// There is no cross-repository Actions API, so this is genuinely one call per
/// repository, and most of each call is `gh` starting up rather than the
/// network. Measured over twenty repositories, sixteen at a time takes about a
/// second and a half where eight took four and a half.
const RUN_FANOUT: usize = 16;

/// Recent workflow runs across `repos`, newest first.
///
/// Failures are dropped rather than fatal: one repository with Actions
/// disabled must not blank out the runs of every other. If every single one
/// fails, the first error is reported instead of a silent empty list.
pub fn all_runs(repos: &[String]) -> Res<Vec<Item>> {
    if repos.is_empty() {
        return Ok(Vec::new());
    }

    let mut results: Vec<Res<Vec<Item>>> = Vec::new();
    for chunk in repos.chunks(RUN_FANOUT) {
        std::thread::scope(|scope| {
            let handles: Vec<_> = chunk
                .iter()
                .map(|repo| scope.spawn(move || (repo.clone(), runs(repo))))
                .collect();
            for h in handles {
                // a panicked worker is treated as a repository that failed
                match h.join() {
                    Ok((repo, Ok(items))) => results.push(Ok(items
                        .into_iter()
                        .map(|mut it| {
                            it.repo = repo.clone();
                            it
                        })
                        .collect())),
                    Ok((_, Err(e))) => results.push(Err(e)),
                    Err(_) => {}
                }
            }
        });
    }

    if results.iter().all(Result::is_err)
        && let Some(Err(e)) = results.pop()
    {
        return Err(e);
    }

    let mut out: Vec<Item> = results.into_iter().flatten().flatten().collect();
    // `when` is a phrase like "3h ago", so the sort needs the ordering the
    // list was built with rather than the text.
    out.sort_by_key(|i| std::cmp::Reverse(i.id));
    Ok(out)
}

/// Clones `repo` into `dest/<name>` and returns where it landed.
///
/// Blocking and slow — minutes for a large repository — which is exactly why
/// it goes through the service thread like everything else.
pub fn clone(repo: &str, dest: &str) -> Res<String> {
    let name = repo.rsplit('/').next().unwrap_or(repo);
    let into = format!("{dest}/{name}");
    run(&["repo", "clone", repo, &into])?;
    Ok(into)
}

// -------------------------------------------------------------------- files

/// A repository's whole file tree, in one call.
///
/// `HEAD` rather than a branch name, so nothing has to know what the default
/// branch is called. Recursive because a tree walked one directory per request
/// makes every keypress wait on the network; a thousand entries arrive in
/// about half a second and navigation is then instant.
pub fn repo_tree(repo: &str) -> Res<Vec<TreeEntry>> {
    let v = json(&["api", &format!("repos/{repo}/git/trees/HEAD?recursive=1")])?;

    let mut out: Vec<TreeEntry> = v
        .get("tree")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|e| TreeEntry {
            path: s(e, "path"),
            is_dir: s(e, "type") == "tree",
            size: e.get("size").and_then(Value::as_u64).unwrap_or(0),
        })
        .collect();

    // GitHub caps a recursive tree; a partial listing that does not say so
    // would read as a repository with fewer files than it has.
    if v.get("truncated").and_then(Value::as_bool).unwrap_or(false) {
        out.push(TreeEntry {
            path: "… truncated by GitHub, not everything is listed".into(),
            is_dir: false,
            size: 0,
        });
    }
    Ok(out)
}

/// How much of a file to fetch. Past this it is not something to read in a
/// pane, and pulling it would only stall the service thread.
pub const FILE_LIMIT: u64 = 512 * 1024;

/// One file's contents.
///
/// Asked for raw rather than as JSON: the contents API otherwise returns
/// base64, and decoding it here would be a decoder to write and get wrong for
/// no gain.
pub fn file_content(repo: &str, path: &str) -> Res<String> {
    let raw = run(&[
        "api",
        &format!("repos/{repo}/contents/{path}"),
        "-H",
        "Accept: application/vnd.github.raw",
    ])?;

    // A binary file comes back as bytes that are not text. Saying so beats
    // painting the pane with replacement characters.
    if raw.contains('\0') {
        return Ok(format!("(binary file, {} bytes)", raw.len()));
    }
    Ok(raw)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "assertions"
)]
mod tests {
    use super::*;

    #[test]
    fn parse_iso_reads_a_utc_timestamp() {
        // 2026-08-13T21:45:34Z
        assert_eq!(parse_iso("2026-08-13T21:45:34Z"), Some(1786657534));
        // the epoch itself, to pin the day-count algorithm
        assert_eq!(parse_iso("1970-01-01T00:00:00Z"), Some(0));
        // leap day, which the month shift has to get right
        assert_eq!(parse_iso("2024-02-29T00:00:00Z"), Some(1709164800));
    }

    #[test]
    fn parse_iso_rejects_anything_it_cannot_read() {
        assert_eq!(parse_iso(""), None);
        assert_eq!(parse_iso("2026-08-13"), None); // too short
        assert_eq!(parse_iso("not-a-date-at-all!!"), None);
    }

    #[test]
    fn duration_handles_the_zero_date_of_unfinished_jobs() {
        // GitHub sends 0001-01-01 for whatever has not finished
        assert_eq!(
            duration("2026-08-13T21:45:34Z", "0001-01-01T00:00:00Z"),
            "—"
        );
        assert_eq!(
            duration("0001-01-01T00:00:00Z", "2026-08-13T21:45:34Z"),
            "—"
        );
        assert_eq!(duration("2026-08-13T21:45:34Z", ""), "—");
    }

    #[test]
    fn duration_formats_minutes_and_seconds() {
        assert_eq!(
            duration("2026-08-13T21:45:34Z", "2026-08-13T21:45:39Z"),
            "5s"
        );
        assert_eq!(
            duration("2026-08-13T21:45:34Z", "2026-08-13T21:47:48Z"),
            "2m 14s"
        );
        // a padded seconds field keeps the columns aligned
        assert_eq!(
            duration("2026-08-13T21:45:34Z", "2026-08-13T21:46:37Z"),
            "1m 03s"
        );
        // an end before the start clamps instead of underflowing
        assert_eq!(
            duration("2026-08-13T21:47:48Z", "2026-08-13T21:45:34Z"),
            "0s"
        );
    }

    #[test]
    fn conclusion_maps_every_state_the_api_can_send() {
        assert_eq!(conclusion("completed", "SUCCESS"), Status::Success);
        assert_eq!(conclusion("completed", "neutral"), Status::Success);
        assert_eq!(conclusion("completed", "TIMED_OUT"), Status::Failure);
        assert_eq!(conclusion("completed", "startup_failure"), Status::Failure);
        assert_eq!(conclusion("completed", "cancelled"), Status::Cancelled);
        assert_eq!(conclusion("completed", "skipped"), Status::Skipped);
        // still running: the conclusion is not filled in yet
        assert_eq!(conclusion("in_progress", ""), Status::Running);
        assert_eq!(conclusion("queued", ""), Status::Pending);
        assert_eq!(conclusion("waiting", ""), Status::Pending);
        // completed with no conclusion at all
        assert_eq!(conclusion("completed", ""), Status::Pending);
    }

    #[test]
    fn strip_ansi_removes_both_escape_forms() {
        // a real ESC byte
        assert_eq!(
            strip_ansi("\u{1b}[36;1mgh pr merge\u{1b}[0m"),
            "gh pr merge"
        );
        // and the caret form that `gh run view --log` writes
        assert_eq!(strip_ansi("^[[36;1mgh pr merge^[[0m"), "gh pr merge");
        // text with no escapes survives untouched
        assert_eq!(strip_ansi("plain text"), "plain text");
        // carriage returns and other controls go away, tabs stay
        assert_eq!(strip_ansi("a\rb\tc\u{7}"), "ab\tc");
    }

    #[test]
    fn strip_ansi_does_not_eat_a_lone_caret() {
        assert_eq!(strip_ansi("2 ^ 8 = 256"), "2 ^ 8 = 256");
        assert_eq!(strip_ansi("^"), "^");
    }

    #[test]
    fn log_kind_classifies_the_lines_actions_emits() {
        assert_eq!(log_kind("##[group]Run cargo test"), "group");
        assert_eq!(log_kind("##[endgroup]"), "group");
        assert_eq!(log_kind("##[error]Process completed"), "red");
        assert_eq!(log_kind("thread 'main' panicked at src/lib.rs:1:1"), "red");
        assert_eq!(log_kind("test foo ... FAILED"), "red");
        assert_eq!(log_kind("error[E0308]: mismatched types"), "red");
        assert_eq!(log_kind("warning: unused variable"), "yellow");
        assert_eq!(log_kind("test foo ... ok"), "green");
        assert_eq!(log_kind("test result: ok. 148 passed"), "green");
        assert_eq!(log_kind("  Compiling serde v1.0"), "fg");
    }

    #[test]
    fn rollup_run_id_digs_the_run_out_of_the_check_url() {
        let v = serde_json::json!({
            "statusCheckRollup": [
                { "detailsUrl": "https://dashboard.gitguardian.com" },
                { "detailsUrl": "https://github.com/o/r/actions/runs/31747058489/job/94604013773" }
            ]
        });
        assert_eq!(rollup_run_id(&v), 31747058489);
    }

    #[test]
    fn rollup_run_id_is_zero_when_no_check_points_at_a_run() {
        let none = serde_json::json!({ "statusCheckRollup": [] });
        assert_eq!(rollup_run_id(&none), 0);

        let external = serde_json::json!({
            "statusCheckRollup": [{ "detailsUrl": "https://example.com/build/7" }]
        });
        assert_eq!(rollup_run_id(&external), 0);

        // the segment is there but what follows is not a number
        let broken = serde_json::json!({
            "statusCheckRollup": [{ "detailsUrl": "https://github.com/o/r/actions/runs/abc" }]
        });
        assert_eq!(rollup_run_id(&broken), 0);
    }

    #[test]
    fn rollup_picks_the_worst_state_of_the_checks() {
        let mixed = serde_json::json!({ "statusCheckRollup": [
            { "status": "COMPLETED", "conclusion": "SUCCESS" },
            { "status": "COMPLETED", "conclusion": "FAILURE" },
        ]});
        assert_eq!(rollup(&mixed), Status::Failure);

        let running = serde_json::json!({ "statusCheckRollup": [
            { "status": "COMPLETED", "conclusion": "SUCCESS" },
            { "status": "IN_PROGRESS", "conclusion": "" },
        ]});
        assert_eq!(rollup(&running), Status::Running);

        let all_good = serde_json::json!({ "statusCheckRollup": [
            { "status": "COMPLETED", "conclusion": "SUCCESS" },
        ]});
        assert_eq!(rollup(&all_good), Status::Success);

        // a PR with no checks at all is pending, not successful
        let empty = serde_json::json!({ "statusCheckRollup": [] });
        assert_eq!(rollup(&empty), Status::Pending);
    }

    #[test]
    fn rollup_understands_legacy_status_contexts() {
        // StatusContext entries carry `state` instead of status/conclusion
        let ctx = serde_json::json!({ "statusCheckRollup": [
            { "state": "FAILURE" },
        ]});
        assert_eq!(rollup(&ctx), Status::Failure);
    }

    #[test]
    fn label_keeps_the_subcommand_and_drops_the_flags() {
        assert_eq!(
            label(&["pr", "list", "-R", "o/r", "--json", "number"]),
            "pr list"
        );
        assert_eq!(label(&["api", "graphql", "-f", "query=..."]), "api graphql");
        assert_eq!(label(&[]), "");
    }

    #[test]
    fn parse_unified_splits_files_and_hunks() {
        let raw = "\
diff --git a/src/one.rs b/src/one.rs
index 111..222 100644
--- a/src/one.rs
+++ b/src/one.rs
@@ -1,3 +1,4 @@
 fn main() {
-    old();
+    new();
+    extra();
 }
diff --git a/src/two.rs b/src/two.rs
@@ -10,2 +10,2 @@ fn helper() {
-    a
+    b
";
        let files = parse_unified(raw);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].0, "src/one.rs");
        assert_eq!(files[1].0, "src/two.rs");

        // the ---/+++ header lines must not be mistaken for content
        assert_eq!(files[0].1.len(), 1);
        let lines = &files[0].1[0].lines;
        assert_eq!(lines.len(), 5);
        assert_eq!(lines[0], (' ', "fn main() {".to_string()));
        assert_eq!(lines[1], ('-', "    old();".to_string()));
        assert_eq!(lines[2], ('+', "    new();".to_string()));
    }

    #[test]
    fn parse_unified_survives_the_awkward_bits() {
        // empty input, no diff at all
        assert!(parse_unified("").is_empty());

        // content before any `diff --git` header is ignored
        assert!(parse_unified("just some noise\n@@ -1 +1 @@\n+x").is_empty());

        // a file with no hunks (a pure rename or mode change)
        let renamed = parse_unified(
            "diff --git a/old.txt b/new.txt\nsimilarity index 100%\nrename from old.txt\n",
        );
        assert_eq!(renamed.len(), 1);
        assert_eq!(renamed[0].0, "new.txt");
        assert!(renamed[0].1.is_empty());

        // the no-newline marker is not a diff line
        let marker = parse_unified(
            "diff --git a/a b/a\n@@ -1 +1 @@\n-a\n+b\n\\ No newline at end of file\n",
        );
        assert_eq!(marker[0].1[0].lines.len(), 2);
    }

    #[test]
    fn parse_unified_keeps_blank_context_lines() {
        // a context line that is only a space arrives as an empty string, and a
        // truly empty line inside a hunk still counts as context
        let raw = "diff --git a/a b/a\n@@ -1,3 +1,3 @@\n a\n\n b\n";
        let files = parse_unified(raw);
        let lines = &files[0].1[0].lines;
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1], (' ', String::new()));
    }
}

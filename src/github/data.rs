//! The design's static data: accounts, repos, issues, PRs, runs, jobs and logs.

/// The state vocabulary the design uses across issues, pull requests and CI.
///
/// One enum rather than two because `Item::state` genuinely holds either,
/// depending on the item's kind — the design's own `sc()` and `si()` map the
/// whole union in a single function too.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Status {
    // issues and pull requests
    Open,
    Draft,
    Merged,
    Closed,
    // checks, jobs and workflow runs
    Success,
    Failure,
    Running,
    Pending,
    Cancelled,
    Skipped,
    /// Anything the API reports that is not modelled here, and the resting
    /// state of a field that does not apply to this kind of item.
    #[default]
    Unknown,
}

impl Status {
    /// The lowercase name the interface prints, matching the design's strings.
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Draft => "draft",
            Self::Merged => "merged",
            Self::Closed => "closed",
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Running => "running",
            Self::Pending => "pending",
            Self::Cancelled => "cancelled",
            Self::Skipped => "skipped",
            Self::Unknown => "",
        }
    }

    /// Reads the API's spelling, which arrives in any case and sometimes with
    /// underscores (`TIMED_OUT`).
    pub fn parse(raw: &str) -> Self {
        match raw.to_lowercase().replace('_', " ").as_str() {
            "open" => Self::Open,
            "draft" => Self::Draft,
            "merged" => Self::Merged,
            "closed" => Self::Closed,
            "success" | "neutral" => Self::Success,
            "failure" | "timed out" | "action required" | "startup failure" => Self::Failure,
            "running" | "in progress" => Self::Running,
            "pending" | "queued" | "waiting" | "requested" => Self::Pending,
            "cancelled" | "canceled" => Self::Cancelled,
            "skipped" => Self::Skipped,
            _ => Self::Unknown,
        }
    }

    /// Is this a pull request that can still be acted on?
    pub fn is_open(self) -> bool {
        matches!(self, Self::Open | Self::Draft)
    }

    /// Has this check, job or run finished running?
    pub fn is_settled(self) -> bool {
        !matches!(self, Self::Running | Self::Pending)
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Issue,
    Pr,
    Run,
}

#[derive(Clone)]
pub struct Repo {
    pub name: String,
    pub private: bool,
    pub lang: String,
    pub issues: u32,
    pub prs: u32,
    pub star: String,
    /// Whether the default branch has a `.github/workflows` directory.
    ///
    /// Carried on the repository because there is no cross-repository Actions
    /// API: the all-repositories runs list is one call per repository, and
    /// this is what keeps it to the handful that could answer.
    pub has_workflows: bool,
}

impl Repo {
    /// The name of the pseudo-repository that stands for all of them.
    ///
    /// A star because GitHub does not allow one in a repository name, so this
    /// can never collide with a real repo and the `owner/name` key that indexes
    /// every cache keeps working untouched.
    pub const ALL: &'static str = "*";

    /// The row that gathers every repository into one list.
    pub fn all(repos: &[Self]) -> Self {
        Self {
            name: Self::ALL.into(),
            private: false,
            lang: String::new(),
            issues: repos.iter().map(|r| r.issues).sum(),
            prs: repos.iter().map(|r| r.prs).sum(),
            star: "0".into(),
            has_workflows: repos.iter().any(|r| r.has_workflows),
        }
    }

    pub fn is_all(&self) -> bool {
        self.name == Self::ALL
    }

    /// What to show for it, since `*` is a key rather than a name.
    pub fn label(&self) -> &str {
        if self.is_all() {
            "all repositories"
        } else {
            &self.name
        }
    }

    /// An empty repo to draw while there is no data yet.
    pub fn empty() -> Self {
        Self {
            name: "—".into(),
            private: false,
            lang: String::new(),
            issues: 0,
            prs: 0,
            star: "0".into(),
            has_workflows: false,
        }
    }
}

#[derive(Clone)]
pub struct Account {
    pub login: String,
    pub kind: String,
    pub sub: String,
    pub repos: Vec<Repo>,
}

/// An issue or pull request label. The colour travels as plain RGB, which is
/// data; turning it into a terminal colour is the view's job.
#[derive(Clone)]
pub struct Label {
    pub name: String,
    pub rgb: (u8, u8, u8),
}

impl Label {
    pub fn new(name: &str, rgb: (u8, u8, u8)) -> Self {
        Self {
            name: name.to_string(),
            rgb,
        }
    }
}

#[derive(Clone)]
pub struct Comment {
    pub author: String,
    pub when: String,
    pub body: String,
}

/// How a reviewer left the pull request. The colour and the glyph are the
/// view's business, not the model's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReviewState {
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
}

impl ReviewState {
    /// The label GitHub itself uses, which is what the detail pane prints.
    pub fn label(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::ChangesRequested => "changes requested",
            Self::Commented => "commented",
            Self::Dismissed => "dismissed",
        }
    }

    /// Parses the API's `APPROVED` / `CHANGES_REQUESTED` / … spelling.
    pub fn parse(raw: &str) -> Self {
        match raw.to_lowercase().replace('_', " ").as_str() {
            "approved" => Self::Approved,
            "changes requested" => Self::ChangesRequested,
            "dismissed" => Self::Dismissed,
            _ => Self::Commented,
        }
    }
}

#[derive(Clone)]
pub struct Review {
    pub author: String,
    pub state: ReviewState,
}

#[derive(Clone)]
pub struct FileChange {
    pub path: String,
    pub add: String,
    pub del: String,
    /// The file's hunks. Empty if the diff has not been fetched yet, or if
    /// there are no textual changes (a mode change, for instance).
    pub hunks: Vec<Hunk>,
}

/// One diff hunk: its `@@ … @@` header and its signed lines.
#[derive(Clone)]
pub struct Hunk {
    pub hdr: String,
    pub lines: Vec<(char, String)>,
}

/// The same shape with static data, which is how the design's diffs arrive.
pub struct StaticHunk {
    pub hdr: &'static str,
    pub lines: &'static [(char, &'static str)],
}

impl From<&StaticHunk> for Hunk {
    fn from(h: &StaticHunk) -> Self {
        Self {
            hdr: h.hdr.to_string(),
            lines: h.lines.iter().map(|(c, t)| (*c, t.to_string())).collect(),
        }
    }
}

impl Hunk {
    /// Expands a file's hunks into numbered lines, deriving both numberings
    /// from the `@@ -o,x +n,y @@` header.
    pub fn rows(hunks: &[Self]) -> Vec<DiffRow> {
        let mut out = Vec::new();
        for h in hunks {
            let (mut o, mut n) = parse_hunk_header(&h.hdr);
            out.push(DiffRow {
                kind: DiffKind::Hdr,
                text: h.hdr.clone(),
                lo: String::new(),
                ln: String::new(),
            });
            for (sign, text) in &h.lines {
                let kind = match sign {
                    '+' => DiffKind::Add,
                    '-' => DiffKind::Del,
                    _ => DiffKind::Ctx,
                };
                let lo = if kind == DiffKind::Add {
                    String::new()
                } else {
                    let s = o.to_string();
                    o += 1;
                    s
                };
                let ln = if kind == DiffKind::Del {
                    String::new()
                } else {
                    let s = n.to_string();
                    n += 1;
                    s
                };
                out.push(DiffRow {
                    kind,
                    text: format!("{sign} {text}"),
                    lo,
                    ln,
                });
            }
        }
        out
    }
}

/// The starting line numbers of a `@@ -186,14 +186,26 @@`.
fn parse_hunk_header(hdr: &str) -> (u32, u32) {
    let num = |tok: &str| -> u32 {
        tok.trim_start_matches(['-', '+'])
            .split(',')
            .next()
            .and_then(|d| d.parse().ok())
            .unwrap_or(1)
    };
    let mut parts = hdr.split_whitespace().skip(1);
    let o = parts.next().map(num).unwrap_or(1);
    let n = parts.next().map(num).unwrap_or(1);
    (o, n)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DiffKind {
    Hdr,
    Add,
    Del,
    Ctx,
}

/// A line ready to draw, with both of its line numbers.
pub struct DiffRow {
    pub kind: DiffKind,
    pub text: String,
    /// Number in the original file; empty for additions.
    pub lo: String,
    /// Number in the new file; empty for deletions.
    pub ln: String,
}

/// The parts of an item that only make sense for one kind. Keeping them inside
/// the variant is what stops an issue from carrying check results, or a run
/// from carrying a branch to delete.
#[derive(Clone)]
pub enum Detail {
    Issue(IssueDetail),
    Pr(Box<PrDetail>),
    Run(RunDetail),
}

#[derive(Clone, Default)]
pub struct IssueDetail {
    pub comments: u32,
    pub comment_list: Vec<Comment>,
}

#[derive(Clone, Default)]
pub struct PrDetail {
    pub checks: Status,
    pub add: String,
    pub del: String,
    pub files: u32,
    pub branch: String,
    pub reviews: Vec<Review>,
    pub file_list: Vec<FileChange>,
    /// Name of the workflow the checks belong to.
    pub workflow: String,
    /// How it was merged, once it has been.
    pub merged_with: Option<String>,
    pub branch_deleted: bool,
}

#[derive(Clone, Default)]
pub struct RunDetail {
    pub event: String,
    pub workflow: String,
    pub dur: String,
}

/// An issue, a pull request or a workflow run. The fields every kind shares
/// live here; the rest are in `detail`.
#[derive(Clone)]
pub struct Item {
    pub num: i64,
    /// Internal identifier (the run's databaseId); 0 for issues.
    pub id: i64,
    /// The `owner/repo` this came from, set only when the list mixes several.
    /// Empty means "the repository the list belongs to", which is the usual
    /// case and keeps every existing fixture unchanged.
    pub repo: String,
    pub title: String,
    pub state: Status,
    pub author: String,
    pub when: String,
    pub body: String,
    pub labels: Vec<Label>,
    pub detail: Detail,
}

impl Item {
    /// A blank item of the given shape. Callers fill in the shared fields and
    /// reach into `detail` for the rest.
    pub fn new(detail: Detail) -> Self {
        Self {
            num: 0,
            id: 0,
            repo: String::new(),
            title: String::new(),
            state: Status::Open,
            author: String::new(),
            when: String::new(),
            body: String::new(),
            labels: Vec::new(),
            detail,
        }
    }

    pub fn issue() -> Self {
        Self::new(Detail::Issue(IssueDetail::default()))
    }

    pub fn pr() -> Self {
        Self::new(Detail::Pr(Box::default()))
    }

    pub fn run() -> Self {
        Self::new(Detail::Run(RunDetail::default()))
    }

    pub fn kind(&self) -> Kind {
        match self.detail {
            Detail::Issue(_) => Kind::Issue,
            Detail::Pr(_) => Kind::Pr,
            Detail::Run(_) => Kind::Run,
        }
    }

    pub fn as_issue(&self) -> Option<&IssueDetail> {
        match &self.detail {
            Detail::Issue(i) => Some(i),
            _ => None,
        }
    }

    pub fn as_issue_mut(&mut self) -> Option<&mut IssueDetail> {
        match &mut self.detail {
            Detail::Issue(i) => Some(i),
            _ => None,
        }
    }

    pub fn as_pr(&self) -> Option<&PrDetail> {
        match &self.detail {
            Detail::Pr(p) => Some(p),
            _ => None,
        }
    }

    pub fn as_pr_mut(&mut self) -> Option<&mut PrDetail> {
        match &mut self.detail {
            Detail::Pr(p) => Some(p),
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn as_run_mut(&mut self) -> Option<&mut RunDetail> {
        match &mut self.detail {
            Detail::Run(r) => Some(r),
            _ => None,
        }
    }

    pub fn as_run(&self) -> Option<&RunDetail> {
        match &self.detail {
            Detail::Run(r) => Some(r),
            _ => None,
        }
    }

    // --- shorthands for the fields the shared render reaches for most ---

    /// Rolled-up check state of a pull request; `Unknown` for anything else.
    pub fn checks(&self) -> Status {
        self.as_pr().map_or(Status::Unknown, |p| p.checks)
    }

    /// Head branch of a pull request; empty for anything else.
    pub fn branch(&self) -> &str {
        self.as_pr().map_or("", |p| p.branch.as_str())
    }

    /// Workflow name, which both a pull request's checks and a run carry.
    pub fn workflow(&self) -> &str {
        match &self.detail {
            Detail::Pr(p) => &p.workflow,
            Detail::Run(r) => &r.workflow,
            Detail::Issue(_) => "",
        }
    }

    /// The changed files of a pull request; empty for anything else.
    pub fn files(&self) -> &[FileChange] {
        self.as_pr().map_or(&[], |p| p.file_list.as_slice())
    }

    /// The design's `cur.body || 'Workflow run triggered by …'`. The filler
    /// only applies to runs, which have no body of their own.
    pub fn body_text(&self) -> String {
        let Detail::Run(run) = &self.detail else {
            return if self.body.is_empty() {
                "no description".to_string()
            } else {
                self.body.clone()
            };
        };
        if self.body.is_empty() {
            let event = if run.event.is_empty() {
                "push"
            } else {
                run.event.as_str()
            };
            format!("Workflow run triggered by {event}.")
        } else {
            self.body.clone()
        }
    }
}

#[derive(Clone)]
pub struct Step {
    pub name: String,
    pub status: Status,
    pub dur: String,
}

#[derive(Clone)]
pub struct Job {
    pub name: String,
    pub status: Status,
    pub dur: String,
    pub steps: Vec<Step>,
}

/// A log line ready to draw.
pub struct LogLine {
    pub time: String,
    pub text: String,
    pub kind: LogKind,
}

/// A demo line: text plus what kind of line it is.
pub type DemoLine = (&'static str, LogKind);

pub struct Tab {
    pub id: &'static str,
    pub label: &'static str,
    pub key: &'static str,
}

/// The repository's file tree.
/// Written by hand, and checked against the table by a test: a `const` that
/// has to agree with a position in a slice is two sources of truth, and
/// reordering `TABS` would move the tab without moving the constant.
pub const FILES_TAB: usize = 3;

/// Index of the Agents tab, which is unlike the others: it is about this
/// machine rather than about a repository, so nothing keyed by `owner/repo`
/// applies to it. Last on purpose, so the repository-scoped tabs stay together.
pub const AGENTS_TAB: usize = 4;

pub const TABS: [Tab; 5] = [
    Tab {
        id: "issues",
        label: "Issues",
        key: "1",
    },
    Tab {
        id: "prs",
        label: "Pull Requests",
        key: "2",
    },
    Tab {
        id: "actions",
        label: "Actions",
        key: "3",
    },
    Tab {
        id: "files",
        label: "Files",
        key: "4",
    },
    Tab {
        id: "agents",
        label: "Agents",
        key: "5",
    },
];

pub const HELP: &[(&str, &str)] = &[
    ("j / k", "move in the pane"),
    ("h / l", "pane left / right"),
    ("g / G", "top / bottom"),
    ("1 2 3", "Issues / PRs / Actions"),
    ("enter", "enter the pane"),
    ("esc / q", "back one level"),
    ("a", "switch account"),
    ("t", "switch theme"),
    ("b", "hide / show repos"),
    ("[ / ]", "previous / next repo"),
    ("p", "find repos, issues, PRs"),
    ("o", "expand / collapse job"),
    ("/", "filter (list or log)"),
    ("e", "jump to first error"),
    ("f", "toggle log follow"),
    ("r", "refresh (fake)"),
    (":", "command line"),
    (":account", "account picker"),
    (":issues :prs", "jump to a tab"),
    (":logs", "open logs of selection"),
    (":diff :files", "open the diff"),
    (":theme", "theme picker"),
    (":sidebar", "hide / show repos"),
    (":find", "the finder"),
    ("?", "this help"),
    (":q", "close overlay"),
    ("^d / ^u", "half page scroll"),
    ("d", "diff of the PR files"),
    ("s", "split / unified diff"),
    ("w", "ignore whitespace"),
    ("tab", "next pane"),
    ("m", "merge the pull request"),
    ("c", "close / reopen the PR"),
    ("D", "delete the branch"),
    ("y / n", "confirm / cancel"),
    ("click", "focus pane, select row"),
    ("2x click", "open it"),
    ("wheel", "scroll under pointer"),
    ("4", "the repo's files"),
    ("5", "agents running here"),
    ("E", "edit the file in $EDITOR"),
    ("^l", "repaint the screen"),
    ("x", "send this to an agent"),
    ("x then type", "…and say something specific"),
];

/// A log line already split by job and step.
pub struct RawLog {
    pub job: String,
    pub step: String,
    pub time: String,
    pub text: String,
    pub kind: LogKind,
}

/// What a line of a log *is*.
///
/// Not what colour it should be, which is what this used to say — the parser
/// returned `"red"` and the view looked `"red"` back up in the palette. A
/// model that names colours has taken a decision that belongs to whoever is
/// drawing, and it takes it in a string nothing checks.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LogKind {
    /// `##[group]` / `##[endgroup]`, the collapsible headings.
    Group,
    /// A failure: `##[error]`, a panic, a failing test.
    Error,
    Warning,
    /// A passing test, or a run that finished clean.
    Success,
    /// The command being run, echoed by the runner.
    Command,
    #[default]
    Plain,
}

/// Converts the raw log into the shape the view consumes.
///
/// The dump's step names do not always match the API's (Actions writes
/// `UNKNOWN STEP` for some), so when the step filter comes back empty we fall
/// back to the whole job log rather than showing nothing.
pub fn filter_log(raw: &[RawLog], job: &str, step: Option<&str>) -> Vec<LogLine> {
    let to_line = |l: &RawLog| LogLine {
        time: l.time.clone(),
        text: l.text.clone(),
        kind: l.kind,
    };
    let same_job = |l: &&RawLog| l.job.eq_ignore_ascii_case(job);

    if let Some(step) = step {
        let picked: Vec<LogLine> = raw
            .iter()
            .filter(same_job)
            .filter(|l| l.step.eq_ignore_ascii_case(step))
            .map(to_line)
            .collect();
        if !picked.is_empty() {
            return picked;
        }
    }
    raw.iter().filter(same_job).map(to_line).collect()
}

/// How a pull request gets merged. A domain concept: the infrastructure
/// layer needs it to build the `gh` flag, and the view to label the choice.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MergeMethod {
    Merge,
    Squash,
    Rebase,
}

pub const MERGE_METHODS: [MergeMethod; 3] =
    [MergeMethod::Merge, MergeMethod::Squash, MergeMethod::Rebase];

impl MergeMethod {
    pub fn label(self) -> &'static str {
        match self {
            Self::Merge => "create a merge commit",
            Self::Squash => "squash and merge",
            Self::Rebase => "rebase and merge",
        }
    }

    /// Short name, the one recorded against the PR.
    pub fn short(self) -> &'static str {
        match self {
            Self::Merge => "merge commit",
            Self::Squash => "squash",
            Self::Rebase => "rebase",
        }
    }
}

impl PrDetail {
    /// How many reviewers approved it.
    ///
    /// Here rather than counted where it is drawn: whether a pull request is
    /// approved is a fact about the pull request, and a view that works it
    /// out is a view that could work it out differently somewhere else.
    pub fn approvals(&self) -> usize {
        self.reviews
            .iter()
            .filter(|r| r.state == ReviewState::Approved)
            .count()
    }

    /// How many asked for changes — the ones that stop a merge.
    pub fn blocking(&self) -> usize {
        self.reviews
            .iter()
            .filter(|r| r.state == ReviewState::ChangesRequested)
            .count()
    }
}

/// How a run's jobs are going.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Tally {
    pub passed: usize,
    pub failed: usize,
    /// Running or waiting for a runner — both are "not done yet" to a reader,
    /// and the summary line has no room for the difference.
    pub in_progress: usize,
}

impl Tally {
    pub fn of(jobs: &[Job]) -> Self {
        let count = |want: &[Status]| jobs.iter().filter(|j| want.contains(&j.status)).count();
        Self {
            passed: count(&[Status::Success]),
            failed: count(&[Status::Failure]),
            in_progress: count(&[Status::Running, Status::Pending]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- what the view used to work out for itself ---

    fn review(state: ReviewState) -> Review {
        Review {
            author: "someone".into(),
            state,
        }
    }

    #[test]
    fn a_pull_request_counts_its_own_approvals() {
        let pr = PrDetail {
            reviews: vec![
                review(ReviewState::Approved),
                review(ReviewState::Approved),
                review(ReviewState::ChangesRequested),
                review(ReviewState::Commented),
            ],
            ..PrDetail::default()
        };
        assert_eq!(pr.approvals(), 2);
        assert_eq!(pr.blocking(), 1, "only a request for changes stops a merge");
    }

    #[test]
    fn a_comment_is_neither_an_approval_nor_a_block() {
        let pr = PrDetail {
            reviews: vec![review(ReviewState::Commented)],
            ..PrDetail::default()
        };
        assert_eq!((pr.approvals(), pr.blocking()), (0, 0));
    }

    fn a_job(status: Status) -> Job {
        Job {
            name: "j".into(),
            status,
            dur: String::new(),
            steps: Vec::new(),
        }
    }

    #[test]
    fn a_tally_counts_waiting_as_in_progress() {
        // Running and waiting for a runner are both "not done yet", and the
        // summary line has no room for the difference.
        let t = Tally::of(&[
            a_job(Status::Success),
            a_job(Status::Success),
            a_job(Status::Failure),
            a_job(Status::Running),
            a_job(Status::Pending),
        ]);
        assert_eq!((t.passed, t.failed, t.in_progress), (2, 1, 2));
    }

    #[test]
    fn a_tally_of_nothing_is_all_zeroes_rather_than_a_panic() {
        assert_eq!(Tally::of(&[]), Tally::default());
    }

    #[test]
    fn a_skipped_job_is_none_of_the_three() {
        // It neither passed nor failed nor is it coming, and counting it as
        // any of those would make the three add up to a lie.
        let t = Tally::of(&[a_job(Status::Skipped), a_job(Status::Cancelled)]);
        assert_eq!((t.passed, t.failed, t.in_progress), (0, 0, 0));
    }

    #[test]
    fn the_tab_constants_point_at_the_tabs_they_are_named_for() {
        // They are positions in `TABS`, written out. Reordering the table
        // would move the tab and leave the constant behind, and the symptom
        // would be the file explorer opening the agents pane.
        assert_eq!(TABS[FILES_TAB].id, "files");
        assert_eq!(TABS[AGENTS_TAB].id, "agents");
    }

    #[test]
    fn no_two_tabs_answer_to_one_id() {
        let mut ids: Vec<&str> = TABS.iter().map(|t| t.id).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "an id is how a tab is picked by name");
    }

    #[test]
    fn filter_log_falls_back_to_the_job_when_the_step_is_unknown() {
        let raw = vec![
            RawLog {
                job: "build".into(),
                step: "Set up job".into(),
                time: "10:00:00".into(),
                text: "one".into(),
                kind: LogKind::Plain,
            },
            RawLog {
                job: "build".into(),
                step: "UNKNOWN STEP".into(),
                time: "10:00:01".into(),
                text: "two".into(),
                kind: LogKind::Plain,
            },
        ];
        // an exact step match wins
        assert_eq!(filter_log(&raw, "build", Some("Set up job")).len(), 1);
        // a step the log never names falls back to the whole job
        assert_eq!(filter_log(&raw, "build", Some("Checkout")).len(), 2);
        // no step filter at all means the whole job
        assert_eq!(filter_log(&raw, "build", None).len(), 2);
        // the job name is matched case-insensitively
        assert_eq!(filter_log(&raw, "BUILD", None).len(), 2);
        // another job matches nothing
        assert_eq!(filter_log(&raw, "test", None).len(), 0);
    }

    #[test]
    fn a_run_falls_back_to_a_generated_body_but_a_pr_does_not() {
        let mut run = Item::run();
        if let Some(d) = run.as_run_mut() {
            d.event = "schedule".into();
        }
        assert_eq!(run.body_text(), "Workflow run triggered by schedule.");

        // a run with no event still reads sensibly
        let bare = Item::run();
        assert_eq!(bare.body_text(), "Workflow run triggered by push.");

        // an empty PR body is an empty PR body, not a workflow message
        let pr = Item::pr();
        assert_eq!(pr.body_text(), "no description");
    }

    #[test]
    fn hunk_header_gives_both_starting_lines() {
        assert_eq!(
            parse_hunk_header("@@ -186,14 +186,26 @@ impl Solver {"),
            (186, 186)
        );
        // a brand new file starts the original side at 0
        assert_eq!(parse_hunk_header("@@ -0,0 +1,12 @@"), (0, 1));
        // single-line hunks omit the count
        assert_eq!(parse_hunk_header("@@ -5 +7 @@"), (5, 7));
    }

    #[test]
    fn hunk_header_defaults_when_it_cannot_be_read() {
        assert_eq!(parse_hunk_header(""), (1, 1));
        assert_eq!(parse_hunk_header("@@"), (1, 1));
        assert_eq!(parse_hunk_header("not a hunk header"), (1, 1));
    }

    fn hunk(hdr: &str, lines: &[(char, &str)]) -> Hunk {
        Hunk {
            hdr: hdr.into(),
            lines: lines.iter().map(|(c, t)| (*c, t.to_string())).collect(),
        }
    }

    #[test]
    fn rows_number_each_side_independently() {
        let h = hunk(
            "@@ -10,3 +10,4 @@",
            &[
                (' ', "ctx"),
                ('-', "gone"),
                ('+', "new"),
                ('+', "more"),
                (' ', "tail"),
            ],
        );
        let rows = Hunk::rows(std::slice::from_ref(&h));

        // header first, then one row per line
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[0].kind, DiffKind::Hdr);
        assert!(rows[0].lo.is_empty() && rows[0].ln.is_empty());

        // context advances both sides
        assert_eq!((rows[1].lo.as_str(), rows[1].ln.as_str()), ("10", "10"));
        // a deletion only advances the original
        assert_eq!((rows[2].lo.as_str(), rows[2].ln.as_str()), ("11", ""));
        // additions only advance the new file
        assert_eq!((rows[3].lo.as_str(), rows[3].ln.as_str()), ("", "11"));
        assert_eq!((rows[4].lo.as_str(), rows[4].ln.as_str()), ("", "12"));
        // and the next context line picks up where each side left off
        assert_eq!((rows[5].lo.as_str(), rows[5].ln.as_str()), ("12", "13"));
    }

    #[test]
    fn rows_prefix_the_sign_onto_the_text() {
        let h = hunk("@@ -1,1 +1,1 @@", &[('+', "added")]);
        let rows = Hunk::rows(std::slice::from_ref(&h));
        assert_eq!(rows[1].text, "+ added");
    }

    #[test]
    fn rows_of_nothing_is_nothing() {
        assert!(Hunk::rows(&[]).is_empty());
        // a hunk with a header but no lines still shows the header
        let empty = hunk("@@ -1 +1 @@", &[]);
        assert_eq!(Hunk::rows(std::slice::from_ref(&empty)).len(), 1);
    }
}

/// One entry of a repository's file tree, as GitHub reports it.
#[derive(Clone)]
pub struct TreeEntry {
    /// Full path from the repository root; the only identity an entry has.
    pub path: String,
    pub is_dir: bool,
    /// Bytes, for a file. Directories report none.
    pub size: u64,
}

impl TreeEntry {
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    pub fn depth(&self) -> usize {
        self.path.matches('/').count()
    }

    /// Every directory this entry sits inside, outermost first.
    ///
    /// What decides whether a row is on screen: an entry is visible only when
    /// all of these have been opened.
    pub fn ancestors(&self) -> Vec<&str> {
        let mut out = Vec::new();
        let mut at = 0;
        while let Some(i) = self.path[at..].find('/') {
            at += i;
            out.push(&self.path[..at]);
            at += 1;
        }
        out
    }
}

// --- what the finder looks through ---
//
// Here rather than with the finder because the worker takes one in a
// request: which kind of thing a search is for is a fact about GitHub,
// not about the modal that happens to ask.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Repos,
    Issues,
    Prs,
    Commits,
}

impl Source {
    pub const ALL: [Self; 4] = [Self::Repos, Self::Issues, Self::Prs, Self::Commits];

    pub fn label(self) -> &'static str {
        match self {
            Self::Repos => "repos",
            Self::Issues => "issues",
            Self::Prs => "pull requests",
            Self::Commits => "commits",
        }
    }

    /// Repositories are filtered here; the rest are searched on GitHub.
    pub fn is_local(self) -> bool {
        self == Self::Repos
    }

    /// GitHub refuses a commit search with no text — qualifiers alone are not
    /// allowed — so that source has nothing to show until something is typed.
    pub fn needs_query(self) -> bool {
        self == Self::Commits
    }

    pub fn placeholder(self) -> &'static str {
        match self {
            Self::Repos => "filter repositories",
            Self::Issues => "search issues in your repositories",
            Self::Prs => "search pull requests in your repositories",
            Self::Commits => "type to search commits — GitHub needs the text",
        }
    }
}

//! What is being handed to an agent, and how much of it.
//!
//! An issue is small enough to send whole. A workflow log is not: a failing
//! run is routinely tens of thousands of lines, almost all of it the same
//! twenty packages resolving. Sending it whole wastes the agent's context on
//! exactly the part nobody reads.
//!
//! So each kind of subject gets its own excerpt, and the excerpt says what it
//! left out. An agent told "1 of 3 errors, 8400 lines omitted" can ask for the
//! rest; one handed a silently truncated log cannot tell it was truncated.

/// Which of the things on screen is being sent.
///
/// Decided by where the reader is rather than chosen from a menu, the same way
/// every other key in this program works: `x` acts on the pane you are in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Subject {
    Issue,
    /// A pull request, with its changed files listed.
    Pr,
    /// A workflow run, with the selected job's log excerpted.
    Run,
    /// One file's diff out of a pull request.
    FileDiff,
    /// A file from the repository, read straight off GitHub.
    File,
}

impl Subject {
    /// What an agent is told about this kind of thing, from the config or
    /// from the default below.
    pub fn template(self) -> String {
        crate::config::template(self.key(), self.default_template())
    }

    /// The config key holding this subject's template.
    pub fn key(self) -> &'static str {
        match self {
            Self::Issue => "prompt",
            Self::Pr => "prompt-pr",
            Self::Run => "prompt-run",
            Self::FileDiff => "prompt-diff",
            Self::File => "prompt-file",
        }
    }

    /// What the picker's header calls it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Issue => "issue",
            Self::Pr => "pull request",
            Self::Run => "failing run",
            Self::FileDiff => "diff",
            Self::File => "file",
        }
    }

    /// The template used when the config says nothing.
    ///
    /// The verb differs on purpose. "Work on this issue" and "diagnose this
    /// run" ask for different things, and the first line of a prompt is what
    /// an agent leans on hardest.
    pub fn default_template(self) -> &'static str {
        match self {
            Self::Issue => "Work on {repo}#{num}: {title}\n\n{url}\n\n---\n\n{context}",
            Self::Pr => {
                "Review {repo}#{num}: {title}\n\n{url}\n\n{context}\n\n\
                 Read the full diff with `gh pr diff {num} -R {repo}`."
            }
            Self::Run => {
                "Diagnose this failing workflow run in {repo}.\n\n\
                 {title}\n{url}\n\n---\n\n{context}"
            }
            Self::FileDiff => {
                "Explain this change from {repo}#{num}: {title}\n\n{url}\n\n---\n\n{context}"
            }
            // No number, because a file is not an issue. The template still
            // takes {num} so someone can add it back if their repository
            // convention wants it.
            Self::File => "Here is a file from {repo}.\n\n{url}\n\n---\n\n{context}",
        }
    }
}

/// How many lines of a log excerpt to send at most.
///
/// Enough for a stack trace and the compile error above it, short enough that
/// it is a fraction of any agent's context rather than all of it.
const LOG_BUDGET: usize = 140;

/// Lines kept either side of an error, so it arrives with what caused it.
const BEFORE: usize = 6;
const AFTER: usize = 3;

/// One line of a log, as far as excerpting is concerned.
pub struct Line<'a> {
    pub text: &'a str,
    pub is_error: bool,
}

/// The part of a log worth sending: the errors and what surrounds them, or
/// the tail if nothing was flagged.
///
/// Always says what it dropped. A truncation an agent cannot see is worse than
/// a shorter excerpt, because it will reason confidently about a log it thinks
/// it has all of.
pub fn log_excerpt(lines: &[Line<'_>]) -> String {
    if lines.is_empty() {
        return "(the log is empty)".to_string();
    }

    let errors: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.is_error)
        .map(|(i, _)| i)
        .collect();

    if errors.is_empty() {
        return tail(lines);
    }
    around_errors(lines, &errors)
}

/// The last of a log that flagged nothing: whatever went wrong is usually at
/// the end.
fn tail(lines: &[Line<'_>]) -> String {
    let start = lines.len().saturating_sub(LOG_BUDGET);
    let mut out = String::new();
    if start > 0 {
        out.push_str(&format!(
            "(no errors flagged; the last {} of {} lines)\n\n",
            LOG_BUDGET,
            lines.len()
        ));
    }
    for l in &lines[start..] {
        out.push_str(l.text);
        out.push('\n');
    }
    out
}

/// The errors with their surroundings, merged where they overlap.
fn around_errors(lines: &[Line<'_>], errors: &[usize]) -> String {
    // Windows first, then merged: two errors three lines apart should read as
    // one passage, not as the same lines printed twice.
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for &i in errors {
        let from = i.saturating_sub(BEFORE);
        let to = (i + AFTER + 1).min(lines.len());
        match spans.last_mut() {
            Some(last) if from <= last.1 => last.1 = last.1.max(to),
            _ => spans.push((from, to)),
        }
    }

    let mut out = String::new();
    let mut used = 0;
    let mut shown = 0;
    let total = spans.len();

    for (n, (from, to)) in spans.iter().enumerate() {
        if used >= LOG_BUDGET {
            break;
        }
        let room = LOG_BUDGET - used;
        let end = (*to).min(from + room);
        if n > 0 || *from > 0 {
            out.push_str(&format!("… (line {})\n", from + 1));
        }
        for l in &lines[*from..end] {
            out.push_str(l.text);
            out.push('\n');
        }
        used += end - from;
        shown += 1;
    }

    // Reported on lines rather than on spans. A log that is nothing but
    // errors merges into a single span, and counting spans would then find
    // nothing missing while silently cutting nineteen hundred lines — which
    // is the one failure mode this whole function exists to avoid.
    if used < lines.len() {
        let total_lines = lines.len();
        out.push_str(&format!("\n… {used} of {total_lines} lines shown"));
        if shown < total {
            out.push_str(&format!(", and {shown} of {total} error passages"));
        }
        out.push('\n');
    }
    out
}

/// How many changed files to name before saying "and N more".
const FILE_BUDGET: usize = 40;

/// The changed files of a pull request, as a list an agent can act on.
pub fn files_summary(files: &[(String, String, String)]) -> String {
    if files.is_empty() {
        return "(the file list has not loaded yet)".to_string();
    }
    let mut out = format!("{} changed file(s):\n", files.len());
    for (path, add, del) in files.iter().take(FILE_BUDGET) {
        out.push_str(&format!("  {path}  {add}/{del}\n"));
    }
    if files.len() > FILE_BUDGET {
        out.push_str(&format!("  … and {} more\n", files.len() - FILE_BUDGET));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(spec: &[(&'static str, bool)]) -> Vec<Line<'static>> {
        spec.iter()
            .map(|(text, is_error)| Line {
                text,
                is_error: *is_error,
            })
            .collect()
    }

    fn plain(n: usize) -> Vec<Line<'static>> {
        // leaked so the borrows live as long as the test needs them
        let texts: Vec<String> = (1..=n).map(|i| format!("line {i}")).collect();
        texts
            .leak()
            .iter()
            .map(|t| Line {
                text: t,
                is_error: false,
            })
            .collect()
    }

    #[test]
    fn an_empty_log_says_so_rather_than_sending_nothing() {
        assert!(log_excerpt(&[]).contains("empty"));
    }

    #[test]
    fn a_short_log_with_no_errors_is_sent_whole() {
        let out = log_excerpt(&plain(10));
        assert!(out.contains("line 1"));
        assert!(out.contains("line 10"));
        assert!(!out.contains("omitted"), "nothing was dropped: {out}");
    }

    #[test]
    fn a_long_log_with_no_errors_keeps_its_tail_and_says_so() {
        let out = log_excerpt(&plain(500));
        assert!(out.contains("line 500"), "the end is what matters");
        assert!(!out.contains("line 1\n"), "the start was dropped");
        assert!(out.contains("of 500 lines"), "and it admits it: {out}");
    }

    #[test]
    fn an_error_arrives_with_what_came_before_it() {
        let mut spec: Vec<(&str, bool)> = (0..20).map(|_| ("noise", false)).collect();
        spec[15] = ("BOOM", true);
        let out = log_excerpt(&lines(&spec));

        assert!(out.contains("BOOM"));
        assert_eq!(
            out.lines().filter(|l| *l == "noise").count(),
            BEFORE + AFTER,
            "the surrounding lines came too"
        );
    }

    #[test]
    fn errors_close_together_read_as_one_passage() {
        let mut spec: Vec<(&str, bool)> = (0..30).map(|_| ("noise", false)).collect();
        spec[10] = ("FIRST", true);
        spec[12] = ("SECOND", true);
        let out = log_excerpt(&lines(&spec));

        assert!(out.contains("FIRST") && out.contains("SECOND"));
        assert_eq!(
            out.matches("noise").count(),
            out.lines().filter(|l| *l == "noise").count(),
            "no line was printed twice"
        );
    }

    #[test]
    fn errors_far_apart_are_separated_and_located() {
        let mut spec: Vec<(&str, bool)> = (0..100).map(|_| ("noise", false)).collect();
        spec[10] = ("FIRST", true);
        spec[80] = ("SECOND", true);
        let out = log_excerpt(&lines(&spec));

        assert!(out.contains("FIRST") && out.contains("SECOND"));
        assert!(out.contains("… (line "), "the gap is marked: {out}");
        assert!(
            out.contains("of 100 lines shown"),
            "and the total is named: {out}"
        );
    }

    #[test]
    fn a_log_of_nothing_but_errors_stops_at_the_budget_and_admits_it() {
        let spec: Vec<(&str, bool)> = (0..2000).map(|_| ("BOOM", true)).collect();
        let out = log_excerpt(&lines(&spec));

        assert!(
            out.lines().count() <= LOG_BUDGET + 4,
            "the budget held: {} lines",
            out.lines().count()
        );
        assert!(
            out.contains("of 2000 lines shown"),
            "and it says what it dropped: {out}"
        );
    }

    #[test]
    fn an_error_on_the_first_line_does_not_underflow() {
        let out = log_excerpt(&lines(&[("BOOM", true), ("after", false)]));
        assert!(out.contains("BOOM"));
    }

    #[test]
    fn an_error_on_the_last_line_does_not_overrun() {
        let out = log_excerpt(&lines(&[("before", false), ("BOOM", true)]));
        assert!(out.contains("BOOM"));
    }

    // --- the file list ---

    #[test]
    fn a_file_list_that_has_not_arrived_says_so() {
        assert!(files_summary(&[]).contains("not loaded"));
    }

    #[test]
    fn every_file_is_named_while_there_are_few() {
        let files = vec![
            ("src/a.rs".into(), "+3".into(), "-1".into()),
            ("src/b.rs".into(), "+0".into(), "-9".into()),
        ];
        let out = files_summary(&files);
        assert!(out.contains("src/a.rs  +3/-1"));
        assert!(out.contains("src/b.rs  +0/-9"));
        assert!(out.starts_with("2 changed file"));
    }

    #[test]
    fn a_large_file_list_is_cut_and_counted() {
        let files: Vec<(String, String, String)> = (0..100)
            .map(|i| (format!("src/f{i}.rs"), "+1".into(), "-1".into()))
            .collect();
        let out = files_summary(&files);
        assert!(out.contains("100 changed file"), "the true count is kept");
        assert!(out.contains(&format!("and {} more", 100 - FILE_BUDGET)));
    }

    #[test]
    fn each_subject_has_its_own_template_and_key() {
        let subjects = [
            Subject::Issue,
            Subject::Pr,
            Subject::Run,
            Subject::FileDiff,
            Subject::File,
        ];
        let keys: Vec<&str> = subjects.iter().map(|s| s.key()).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), keys.len(), "the config keys are distinct");

        for s in subjects {
            let t = s.default_template();
            assert!(t.contains("{repo}"), "{s:?} names the repository");
            assert!(t.contains("{context}"), "{s:?} carries its content");
        }
    }
}

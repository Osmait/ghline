//! How long ago something was, in words.
//!
//! Shared rather than living with the GitHub layer: a blame line beside a diff
//! should read the same as a timestamp beside a pull request, and diffline has
//! no business importing `gh` to borrow a phrase.

/// How long ago a unix timestamp was.
///
/// Coarsens as it goes back — minutes, then hours, then days, then weeks — and
/// stops there: a year expressed in weeks is still readable, and every unit
/// past a week has to decide how long a month is.
///
/// ```rust
/// use github_tui::shared::ago::since;
///
/// let now = std::time::SystemTime::now()
///     .duration_since(std::time::UNIX_EPOCH)
///     .map_or(0, |d| d.as_secs() as i64);
/// assert_eq!(since(now - 7200), "2h ago");
/// assert_eq!(since(now + 600), "just now", "a server clock ahead of ours");
/// ```
#[must_use]
pub fn since(then: i64) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    #[test]
    fn the_units_change_as_it_gets_older() {
        assert_eq!(since(now() - 5), "just now");
        assert_eq!(since(now() - 300), "5m ago");
        assert_eq!(since(now() - 7200), "2h ago");
        assert_eq!(since(now() - 172_800), "2d ago");
        assert_eq!(since(now() - 1_209_600), "2w ago");
    }

    #[test]
    fn a_time_in_the_future_is_not_negative() {
        // clock skew between a server and this machine is ordinary
        assert_eq!(since(now() + 600), "just now");
    }
}

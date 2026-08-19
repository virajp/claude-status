//! Case-insensitive pattern matching for `worktreePattern` and (plan 2)
//! `subagent.statuses[].match`.
//!
//! Every shipped pattern is a plain alternation of literals — `worktree`,
//! `done|complete|success|finish|ok`. Those compile to a lowercased substring
//! search, which needs no engine at all. Anything with real syntax in it falls
//! back to `regex-lite`.

/// Characters that mean something to a regex engine. `|` is absent on purpose:
/// alternation is what the fast path implements.
const METACHARACTERS: &[char] = &['.', '^', '$', '*', '+', '?', '(', ')', '[', ']', '{', '}', '\\'];

pub enum Matcher {
    /// Lowercased alternatives, matched by substring.
    Literals(Vec<String>),
    Regex(regex_lite::Regex),
}

impl Matcher {
    /// Compiles a pattern, or reports why `regex-lite` rejected it.
    pub fn compile(pattern: &str) -> Result<Self, regex_lite::Error> {
        if !pattern.contains(METACHARACTERS) {
            return Ok(Self::Literals(pattern.split('|').map(str::to_lowercase).collect()));
        }
        regex_lite::RegexBuilder::new(pattern).case_insensitive(true).build().map(Self::Regex)
    }

    pub fn is_match(&self, haystack: &str) -> bool {
        match self {
            // An empty pattern matches everything, as `new RegExp("")` did —
            // which is what makes an empty `match` a usable fallback entry.
            Self::Literals(alts) => {
                let haystack = haystack.to_lowercase();
                alts.iter().any(|alt| haystack.contains(alt.as_str()))
            }
            Self::Regex(re) => re.is_match(haystack),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(pattern: &str, haystack: &str) -> bool {
        Matcher::compile(pattern).expect("compiles").is_match(haystack)
    }

    #[test]
    fn a_bare_literal_matches_case_insensitively_as_a_substring() {
        assert!(matches("worktree", "/Users/x/repo/.worktrees/main-bar"));
        assert!(matches("worktree", "WORKTREE"));
        assert!(!matches("worktree", "/Users/x/repo/src"));
    }

    #[test]
    fn an_alternation_takes_the_fast_path() {
        assert!(matches!(Matcher::compile("done|complete|ok"), Ok(Matcher::Literals(_))));
        assert!(matches("done|complete|success|finish|ok", "Completed"));
        assert!(matches("run|active|progress", "IN_PROGRESS"));
        assert!(!matches("run|active|progress", "queued"));
    }

    #[test]
    fn an_empty_pattern_matches_everything() {
        assert!(matches("", "anything"));
        assert!(matches("", ""));
    }

    #[test]
    fn a_pattern_with_syntax_falls_through_to_the_engine() {
        assert!(matches!(Matcher::compile(r"^work.*ree$"), Ok(Matcher::Regex(_))));
        assert!(matches(r"^work.*ree$", "WORKTREE"), "the engine is case-insensitive too");
        assert!(!matches(r"^work.*ree$", "my-worktree-dir"));
        assert!(matches(r"tree\d+", "worktree42"));
    }

    #[test]
    fn an_invalid_pattern_is_an_error_rather_than_a_panic() {
        assert!(Matcher::compile("(unclosed").is_err());
    }
}

use std::fmt;
use std::sync::Arc;

use regex::Regex;

#[derive(Clone)]
pub struct RegexValue {
    pattern: String,
    regex: Arc<Regex>,
}

impl RegexValue {
    pub fn try_new(pattern: impl Into<String>) -> Result<Self, regex::Error> {
        let pattern = pattern.into();
        let regex = Regex::new(&pattern)?;
        Ok(Self {
            pattern,
            regex: Arc::new(regex),
        })
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    pub fn regex(&self) -> &Regex {
        &self.regex
    }
}

impl fmt::Debug for RegexValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Regex").field(&self.pattern).finish()
    }
}

impl PartialEq for RegexValue {
    fn eq(&self, other: &Self) -> bool {
        self.pattern == other.pattern
    }
}

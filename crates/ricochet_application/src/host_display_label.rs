use std::fmt;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HostDisplayLabel(String);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct HostDisplayLabelError;

impl HostDisplayLabel {
    pub fn parse(value: &str) -> Result<Self, HostDisplayLabelError> {
        if !(1..=160).contains(&value.len()) || value.chars().any(is_forbidden_control) {
            return Err(HostDisplayLabelError);
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_forbidden_control(character: char) -> bool {
    matches!(
        character,
        '\u{0000}'..='\u{001f}'
            | '\u{007f}'..='\u{009f}'
            | '\u{061c}'
            | '\u{200e}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
    )
}

impl fmt::Debug for HostDisplayLabel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("HostDisplayLabel")
            .field(&self.0)
            .finish()
    }
}

impl fmt::Display for HostDisplayLabel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl fmt::Debug for HostDisplayLabelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("HostDisplayLabelError")
    }
}

impl fmt::Display for HostDisplayLabelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid host display label")
    }
}

impl std::error::Error for HostDisplayLabelError {}

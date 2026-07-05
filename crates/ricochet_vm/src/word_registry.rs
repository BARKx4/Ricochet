#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordCategory {
    Core,
    Collection,
    Result,
    Host,
    WebView,
    Agent,
    Debug,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityRequirement {
    None,
    Filesystem,
    FilesystemWrite,
    Http,
    Socket,
    Process,
    Pty,
    Tui,
    WebView,
    Environment,
    Sleep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordMetadata {
    pub name: &'static str,
    pub category: WordCategory,
    pub capability: CapabilityRequirement,
    pub stack: &'static str,
    pub summary: &'static str,
}

pub const BUILTIN_WORDS: &[WordMetadata] = &[
    WordMetadata {
        name: "runtime_capabilities",
        category: WordCategory::Host,
        capability: CapabilityRequirement::None,
        stack: "-- map",
        summary: "Returns the active host capability map.",
    },
    WordMetadata {
        name: "webview_window_app",
        category: WordCategory::WebView,
        capability: CapabilityRequirement::WebView,
        stack: "title body state actions menu_bar -- result",
        summary: "Builds a desktop WebView app document with app-kit metadata.",
    },
    WordMetadata {
        name: "web_command",
        category: WordCategory::WebView,
        capability: CapabilityRequirement::WebView,
        stack: "id label shortcut -- command",
        summary: "Creates a WebView/native-menu command descriptor.",
    },
    WordMetadata {
        name: "approval_create",
        category: WordCategory::Agent,
        capability: CapabilityRequirement::None,
        stack: "operation options -- result",
        summary: "Creates an exactly-once approval record and one-time claim token.",
    },
    WordMetadata {
        name: "process_start",
        category: WordCategory::Agent,
        capability: CapabilityRequirement::Process,
        stack: "command args options -- result",
        summary: "Starts a retained host process under the active process policy.",
    },
];

pub fn builtin_words() -> &'static [WordMetadata] {
    BUILTIN_WORDS
}

pub fn builtin_word(name: &str) -> Option<&'static WordMetadata> {
    BUILTIN_WORDS.iter().find(|word| word.name == name)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn seeded_word_registry_has_unique_names() {
        let mut names = BTreeSet::new();
        for word in builtin_words() {
            assert!(
                names.insert(word.name),
                "duplicate word metadata for {}",
                word.name
            );
            assert!(
                !word.stack.trim().is_empty(),
                "{} missing stack signature",
                word.name
            );
            assert!(
                !word.summary.trim().is_empty(),
                "{} missing summary",
                word.name
            );
        }
    }

    #[test]
    fn mvp_critical_words_are_registered() {
        for name in [
            "runtime_capabilities",
            "webview_window_app",
            "web_command",
            "approval_create",
            "process_start",
        ] {
            assert!(builtin_word(name).is_some(), "{name} is not registered");
        }
    }
}

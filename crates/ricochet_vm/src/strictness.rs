#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrictnessDiagnosticKind {
    UnknownQuestionWordFallback,
    NilProducingLookup,
    MissingProductionSessionSecret,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrictnessDiagnostic {
    pub kind: StrictnessDiagnosticKind,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct StrictnessConfig {
    pub warn_unknown_question_word_fallback: bool,
    pub warn_nil_producing_lookup: bool,
}

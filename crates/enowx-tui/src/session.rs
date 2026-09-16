#[derive(Clone)]
pub(crate) enum TranscriptKind {
    User,
    Assistant,
    Reasoning,
    Tool {
        id: String,
        name: String,
        args: String,
        result: String,
        running: bool,
        error: bool,
    },
    Notice,
    Error,
    System,
}

#[derive(Clone)]
pub(crate) struct TranscriptBlock {
    pub(crate) kind: TranscriptKind,
    pub(crate) text: String,
}
/// What the running turn is doing right now. The UI animates this so a long
/// model call is visibly alive instead of looking hung.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Activity {
    Idle,
    Waiting,
    Thinking,
    Writing,
    Tool(String),
}

impl Activity {
    pub(crate) fn label(&self) -> &str {
        match self {
            Activity::Idle => "ready",
            Activity::Waiting => "waiting for the model",
            Activity::Thinking => "thinking",
            Activity::Writing => "writing",
            Activity::Tool(name) => name,
        }
    }
}

pub(crate) const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    FileSystem,
    Http,
    Process,
    Terminal,
    Webview,
}

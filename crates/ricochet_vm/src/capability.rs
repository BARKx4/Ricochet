#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    FileSystem,
    Http,
    Terminal,
    Webview,
}

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use ricochet_application::HostDisplayLabel;
use zeroize::Zeroizing;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PromptPlatformContract {
    control_class: &'static str,
    masked: bool,
}

impl PromptPlatformContract {
    pub const WINDOWS: Self = Self {
        control_class: "Win32 EDIT",
        masked: true,
    };
    pub const MACOS: Self = Self {
        control_class: "NSSecureTextField",
        masked: true,
    };
    pub const LINUX: Self = Self {
        control_class: "GTK3 gtk::Entry",
        masked: true,
    };

    pub fn control_class(self) -> &'static str {
        self.control_class
    }

    pub fn masked(self) -> bool {
        self.masked
    }
}

pub struct NativePromptRequest {
    ticket: u64,
    label: HostDisplayLabel,
    canonical_path: String,
}

pub enum NativePromptOutcome {
    Stored(Zeroizing<String>),
    Cancelled,
}

pub struct NativePromptResult {
    ticket: u64,
    outcome: NativePromptOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativePromptErrorKind {
    WrongThread,
    NativeControl,
    InvalidValue,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct NativePromptError {
    kind: NativePromptErrorKind,
}

pub trait NativePromptControl: Send + Sync {
    fn prompt(
        &self,
        request: &NativePromptRequest,
    ) -> Result<NativePromptOutcome, NativePromptError>;
}

#[derive(Clone)]
pub struct NativePromptDispatcher {
    control: Arc<dyn NativePromptControl>,
    focus_restored: Arc<AtomicBool>,
}

pub struct HostPromptCoordinator {
    state: Mutex<PromptCoordinatorState>,
    ready: Condvar,
}

struct PromptCoordinatorState {
    next_ticket: u64,
    active: bool,
}

struct PlatformPromptControl {
    parent: NativePromptParent,
    gui_thread: std::thread::ThreadId,
}

#[derive(Clone, Copy)]
pub(crate) struct NativePromptParent {
    #[cfg_attr(not(windows), allow(dead_code))]
    raw: isize,
}

impl NativePromptRequest {
    pub fn new(ticket: u64, label: HostDisplayLabel, canonical_path: impl Into<String>) -> Self {
        Self {
            ticket,
            label,
            canonical_path: canonical_path.into(),
        }
    }

    pub fn ticket(&self) -> u64 {
        self.ticket
    }

    pub fn label(&self) -> &HostDisplayLabel {
        &self.label
    }

    pub fn canonical_path(&self) -> &str {
        &self.canonical_path
    }
}

impl NativePromptResult {
    pub fn ticket(&self) -> u64 {
        self.ticket
    }

    pub fn outcome(&self) -> &NativePromptOutcome {
        &self.outcome
    }

    pub fn into_outcome(self) -> NativePromptOutcome {
        self.outcome
    }
}

impl NativePromptError {
    pub(crate) fn new(kind: NativePromptErrorKind) -> Self {
        Self { kind }
    }

    pub fn kind(&self) -> NativePromptErrorKind {
        self.kind
    }

    pub fn stable_code(&self) -> &'static str {
        "secure_prompt_failed"
    }
}

impl NativePromptDispatcher {
    pub fn from_control(control: Arc<dyn NativePromptControl>) -> Self {
        Self {
            control,
            focus_restored: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn platform(parent: NativePromptParent) -> Self {
        Self::from_control(Arc::new(PlatformPromptControl {
            parent,
            gui_thread: std::thread::current().id(),
        }))
    }

    fn prompt(
        &self,
        request: &NativePromptRequest,
    ) -> Result<NativePromptOutcome, NativePromptError> {
        self.focus_restored.store(false, Ordering::Release);
        let result = self.control.prompt(request);
        self.focus_restored.store(true, Ordering::Release);
        result
    }

    pub fn focus_restored(&self) -> bool {
        self.focus_restored.load(Ordering::Acquire)
    }
}

impl HostPromptCoordinator {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(PromptCoordinatorState {
                next_ticket: 1,
                active: false,
            }),
            ready: Condvar::new(),
        }
    }

    pub fn prompt(
        &self,
        dispatcher: &NativePromptDispatcher,
        request: NativePromptRequest,
    ) -> Result<NativePromptResult, NativePromptError> {
        let ticket = request.ticket;
        let mut state = self
            .state
            .lock()
            .expect("host prompt coordinator lock poisoned");
        while state.active || ticket != state.next_ticket {
            state = self
                .ready
                .wait(state)
                .expect("host prompt coordinator lock poisoned");
        }
        state.active = true;
        drop(state);

        let outcome = dispatcher.prompt(&request);

        let mut state = self
            .state
            .lock()
            .expect("host prompt coordinator lock poisoned");
        state.active = false;
        state.next_ticket = state.next_ticket.saturating_add(1);
        self.ready.notify_all();
        drop(state);
        outcome.map(|outcome| NativePromptResult { ticket, outcome })
    }
}

impl Default for HostPromptCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl NativePromptParent {
    pub(crate) fn from_raw(raw: isize) -> Self {
        Self { raw }
    }
}

impl NativePromptControl for PlatformPromptControl {
    fn prompt(
        &self,
        request: &NativePromptRequest,
    ) -> Result<NativePromptOutcome, NativePromptError> {
        if std::thread::current().id() != self.gui_thread {
            return Err(NativePromptError::new(NativePromptErrorKind::WrongThread));
        }
        #[cfg(windows)]
        return windows::prompt(request, self.parent);
        #[cfg(target_os = "macos")]
        return macos::prompt(request, self.parent);
        #[cfg(target_os = "linux")]
        return linux::prompt(request, self.parent);
        #[allow(unreachable_code)]
        Err(NativePromptError::new(NativePromptErrorKind::NativeControl))
    }
}

impl fmt::Debug for NativePromptOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stored(_) => formatter.write_str("Stored(<zeroizing-secret>)"),
            Self::Cancelled => formatter.write_str("Cancelled"),
        }
    }
}

impl fmt::Debug for NativePromptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativePromptError")
            .field("kind", &self.kind)
            .finish()
    }
}

impl fmt::Display for NativePromptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.stable_code())
    }
}

impl std::error::Error for NativePromptError {}

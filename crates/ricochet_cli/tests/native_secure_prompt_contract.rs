use std::sync::{Arc, Mutex};
use std::thread;

use ricochet_application::HostDisplayLabel;
use ricochet_cli::secure_prompt::{
    HostPromptCoordinator, NativePromptControl, NativePromptDispatcher, NativePromptOutcome,
    NativePromptRequest, PromptPlatformContract,
};
use zeroize::Zeroizing;

#[derive(Default)]
struct SyntheticControl {
    responses: Mutex<Vec<NativePromptOutcome>>,
    seen: Mutex<Vec<String>>,
}

impl NativePromptControl for SyntheticControl {
    fn prompt(
        &self,
        request: &NativePromptRequest,
    ) -> Result<NativePromptOutcome, ricochet_cli::secure_prompt::NativePromptError> {
        self.seen
            .lock()
            .expect("seen lock")
            .push(request.canonical_path().to_string());
        Ok(self.responses.lock().expect("responses lock").remove(0))
    }
}

#[test]
fn secure_session_prompt_platform_contracts_are_native_and_masked() {
    assert_eq!(
        PromptPlatformContract::WINDOWS.control_class(),
        "Win32 EDIT"
    );
    assert!(PromptPlatformContract::WINDOWS.masked());
    assert_eq!(
        PromptPlatformContract::MACOS.control_class(),
        "NSSecureTextField"
    );
    assert!(PromptPlatformContract::MACOS.masked());
    assert_eq!(
        PromptPlatformContract::LINUX.control_class(),
        "GTK3 gtk::Entry"
    );
    assert!(PromptPlatformContract::LINUX.masked());

    let windows = include_str!("../src/secure_prompt/windows.rs");
    let macos = include_str!("../src/secure_prompt/macos.rs");
    let linux = include_str!("../src/secure_prompt/linux.rs");
    assert!(windows.contains("ES_PASSWORD"));
    assert!(macos.contains("NSSecureTextField"));
    assert!(linux.contains("gtk::Entry"));
    assert!(linux.contains("set_visibility(false)"));
}

#[test]
fn secure_session_prompt_serializes_workers_and_preserves_ticket_order() {
    let control = Arc::new(SyntheticControl {
        responses: Mutex::new(vec![
            NativePromptOutcome::Stored(Zeroizing::new("first-synthetic".to_string())),
            NativePromptOutcome::Cancelled,
        ]),
        seen: Mutex::new(Vec::new()),
    });
    let coordinator = Arc::new(HostPromptCoordinator::new());
    let dispatcher = Arc::new(NativePromptDispatcher::from_control(control.clone()));
    let first = NativePromptRequest::new(
        1,
        HostDisplayLabel::parse("OpenAI session key").expect("label"),
        "callback-gui/provider.openai",
    );
    let second = NativePromptRequest::new(
        2,
        HostDisplayLabel::parse("Anthropic session key").expect("label"),
        "callback-gui/provider.anthropic",
    );

    let first_worker = {
        let coordinator = Arc::clone(&coordinator);
        let dispatcher = Arc::clone(&dispatcher);
        thread::spawn(move || coordinator.prompt(&dispatcher, first))
    };
    let second_worker = {
        let coordinator = Arc::clone(&coordinator);
        let dispatcher = Arc::clone(&dispatcher);
        thread::spawn(move || coordinator.prompt(&dispatcher, second))
    };
    let first_result = first_worker
        .join()
        .expect("first worker")
        .expect("first prompt");
    let second_result = second_worker
        .join()
        .expect("second worker")
        .expect("second prompt");
    assert_eq!(first_result.ticket(), 1);
    assert_eq!(second_result.ticket(), 2);
    assert!(matches!(
        first_result.outcome(),
        NativePromptOutcome::Stored(_)
    ));
    assert!(matches!(
        second_result.outcome(),
        NativePromptOutcome::Cancelled
    ));
    assert_eq!(control.seen.lock().expect("seen lock").len(), 2);
    assert!(dispatcher.focus_restored());
}

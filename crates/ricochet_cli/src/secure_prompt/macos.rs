use objc2::rc::Retained;
use objc2_app_kit::{NSAlert, NSAlertFirstButtonReturn, NSSecureTextField};
use objc2_foundation::{MainThreadMarker, NSString};
use zeroize::Zeroizing;

use super::{
    NativePromptError, NativePromptErrorKind, NativePromptOutcome, NativePromptParent,
    NativePromptRequest,
};

pub(super) fn prompt(
    request: &NativePromptRequest,
    _parent: NativePromptParent,
) -> Result<NativePromptOutcome, NativePromptError> {
    let mtm = MainThreadMarker::new()
        .ok_or_else(|| NativePromptError::new(NativePromptErrorKind::WrongThread))?;
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(request.label().as_str()));
    alert.setInformativeText(&NSString::from_str(request.canonical_path()));
    alert.addButtonWithTitle(&NSString::from_str("Store for this session"));
    alert.addButtonWithTitle(&NSString::from_str("Cancel"));
    let field: Retained<NSSecureTextField> = NSSecureTextField::new(mtm);
    alert.setAccessoryView(Some(&field));
    let response = alert.runModal();
    if response != NSAlertFirstButtonReturn {
        return Ok(NativePromptOutcome::Cancelled);
    }
    let value = Zeroizing::new(field.stringValue().to_string());
    field.setStringValue(&NSString::from_str(""));
    if value.is_empty() || value.len() > 2048 {
        return Err(NativePromptError::new(NativePromptErrorKind::InvalidValue));
    }
    Ok(NativePromptOutcome::Stored(value))
}

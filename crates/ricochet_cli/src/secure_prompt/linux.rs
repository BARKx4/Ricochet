use gtk::prelude::*;
use zeroize::Zeroizing;

use super::{
    NativePromptError, NativePromptErrorKind, NativePromptOutcome, NativePromptParent,
    NativePromptRequest,
};

pub(super) fn prompt(
    request: &NativePromptRequest,
    _parent: NativePromptParent,
) -> Result<NativePromptOutcome, NativePromptError> {
    gtk::init().map_err(|_| NativePromptError::new(NativePromptErrorKind::NativeControl))?;
    let dialog = gtk::Dialog::with_buttons(
        Some("Ricochet secure session credential"),
        None::<&gtk::Window>,
        gtk::DialogFlags::MODAL,
        &[
            ("Cancel", gtk::ResponseType::Cancel),
            ("Store for this session", gtk::ResponseType::Ok),
        ],
    );
    let label = gtk::Label::new(Some(request.label().as_str()));
    let path = gtk::Label::new(Some(request.canonical_path()));
    let entry = gtk::Entry::new();
    entry.set_visibility(false);
    dialog.content_area().add(&label);
    dialog.content_area().add(&path);
    dialog.content_area().add(&entry);
    dialog.show_all();
    let response = dialog.run();
    let outcome = if response == gtk::ResponseType::Ok {
        let value = Zeroizing::new(entry.text().to_string());
        if value.is_empty() || value.len() > 2048 {
            Err(NativePromptError::new(NativePromptErrorKind::InvalidValue))
        } else {
            Ok(NativePromptOutcome::Stored(value))
        }
    } else {
        Ok(NativePromptOutcome::Cancelled)
    };
    entry.set_text("");
    dialog.close();
    outcome
}

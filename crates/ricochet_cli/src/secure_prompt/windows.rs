use std::ffi::c_void;
use std::mem;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, RegisterClassW, SetForegroundWindow,
    SetWindowLongPtrW, ShowWindow, TranslateMessage, BN_CLICKED, BS_DEFPUSHBUTTON, CREATESTRUCTW,
    CW_USEDEFAULT, ES_AUTOHSCROLL, ES_PASSWORD, GWLP_USERDATA, MSG, SW_SHOW, WM_CLOSE, WM_COMMAND,
    WM_NCCREATE, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP,
    WS_VISIBLE,
};
use zeroize::Zeroizing;

use super::{
    NativePromptError, NativePromptErrorKind, NativePromptOutcome, NativePromptParent,
    NativePromptRequest,
};

const OK_ID: usize = 1;
const CANCEL_ID: usize = 2;

struct DialogState {
    edit: HWND,
    done: bool,
    outcome: Option<Result<NativePromptOutcome, NativePromptError>>,
}

pub(super) fn prompt(
    request: &NativePromptRequest,
    parent: NativePromptParent,
) -> Result<NativePromptOutcome, NativePromptError> {
    unsafe { prompt_inner(request, parent) }
}

unsafe fn prompt_inner(
    request: &NativePromptRequest,
    parent: NativePromptParent,
) -> Result<NativePromptOutcome, NativePromptError> {
    let class_name = wide("RicochetSecureSessionPrompt");
    let instance = GetModuleHandleW(null());
    if instance.is_null() {
        return Err(native_error());
    }
    let class = WNDCLASSW {
        lpfnWndProc: Some(dialog_proc),
        hInstance: instance,
        lpszClassName: class_name.as_ptr(),
        ..Default::default()
    };
    let _ = RegisterClassW(&class);

    let mut state = DialogState {
        edit: null_mut(),
        done: false,
        outcome: None,
    };
    let title = wide("Ricochet secure session credential");
    let parent_hwnd = parent.raw as HWND;
    let window = CreateWindowExW(
        0,
        class_name.as_ptr(),
        title.as_ptr(),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        560,
        230,
        parent_hwnd,
        null_mut(),
        instance,
        (&mut state as *mut DialogState).cast::<c_void>(),
    );
    if window.is_null() {
        return Err(native_error());
    }

    let label = wide(request.label().as_str());
    let canonical_path = wide(request.canonical_path());
    let empty = wide("");
    let static_class = wide("STATIC");
    let edit_class = wide("EDIT");
    let button_class = wide("BUTTON");
    let ok = wide("Store for this session");
    let cancel = wide("Cancel");
    if CreateWindowExW(
        0,
        static_class.as_ptr(),
        label.as_ptr(),
        WS_CHILD | WS_VISIBLE,
        20,
        18,
        510,
        24,
        window,
        null_mut(),
        instance,
        null(),
    )
    .is_null()
        || CreateWindowExW(
            0,
            static_class.as_ptr(),
            canonical_path.as_ptr(),
            WS_CHILD | WS_VISIBLE,
            20,
            46,
            510,
            24,
            window,
            null_mut(),
            instance,
            null(),
        )
        .is_null()
    {
        DestroyWindow(window);
        return Err(native_error());
    }
    state.edit = CreateWindowExW(
        0,
        edit_class.as_ptr(),
        empty.as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_BORDER | WS_TABSTOP | ES_PASSWORD as u32 | ES_AUTOHSCROLL as u32,
        20,
        82,
        510,
        28,
        window,
        null_mut(),
        instance,
        null(),
    );
    let ok_button = CreateWindowExW(
        0,
        button_class.as_ptr(),
        ok.as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
        260,
        132,
        170,
        30,
        window,
        OK_ID as *mut c_void,
        instance,
        null(),
    );
    let cancel_button = CreateWindowExW(
        0,
        button_class.as_ptr(),
        cancel.as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        440,
        132,
        90,
        30,
        window,
        CANCEL_ID as *mut c_void,
        instance,
        null(),
    );
    if state.edit.is_null() || ok_button.is_null() || cancel_button.is_null() {
        DestroyWindow(window);
        return Err(native_error());
    }
    ShowWindow(window, SW_SHOW);

    let mut message: MSG = mem::zeroed();
    while !state.done {
        let status = GetMessageW(&mut message, null_mut(), 0, 0);
        if status <= 0 {
            state.outcome = Some(Err(native_error()));
            state.done = true;
            break;
        }
        TranslateMessage(&message);
        DispatchMessageW(&message);
    }
    DestroyWindow(window);
    if !parent_hwnd.is_null() {
        SetForegroundWindow(parent_hwnd);
    }
    state.outcome.unwrap_or(Ok(NativePromptOutcome::Cancelled))
}

unsafe extern "system" fn dialog_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = lparam as *const CREATESTRUCTW;
        if !create.is_null() {
            SetWindowLongPtrW(window, GWLP_USERDATA, (*create).lpCreateParams as isize);
        }
    }
    let state = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut DialogState;
    if !state.is_null() {
        if message == WM_CLOSE {
            (*state).outcome = Some(Ok(NativePromptOutcome::Cancelled));
            (*state).done = true;
            return 0;
        }
        if message == WM_COMMAND && ((wparam >> 16) as u32) == BN_CLICKED {
            match wparam & 0xffff {
                OK_ID => {
                    (*state).outcome = Some(read_edit((*state).edit));
                    (*state).done = true;
                    return 0;
                }
                CANCEL_ID => {
                    (*state).outcome = Some(Ok(NativePromptOutcome::Cancelled));
                    (*state).done = true;
                    return 0;
                }
                _ => {}
            }
        }
    }
    DefWindowProcW(window, message, wparam, lparam)
}

unsafe fn read_edit(edit: HWND) -> Result<NativePromptOutcome, NativePromptError> {
    let length = GetWindowTextLengthW(edit);
    if !(1..=2048).contains(&length) {
        return Err(NativePromptError::new(NativePromptErrorKind::InvalidValue));
    }
    let mut buffer = zeroize::Zeroizing::new(vec![0_u16; length as usize + 1]);
    let copied = GetWindowTextW(edit, buffer.as_mut_ptr(), buffer.len() as i32);
    if copied != length {
        return Err(native_error());
    }
    let value = String::from_utf16(&buffer[..copied as usize]).map_err(|_| native_error())?;
    if value.is_empty() || value.len() > 2048 {
        return Err(NativePromptError::new(NativePromptErrorKind::InvalidValue));
    }
    Ok(NativePromptOutcome::Stored(Zeroizing::new(value)))
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn native_error() -> NativePromptError {
    NativePromptError::new(NativePromptErrorKind::NativeControl)
}

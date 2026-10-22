//! Detects physical mouse activity of the user sitting at the controlled computer.
//!
//! Input injected by RustDesk is tagged with `enigo::ENIGO_INPUT_EXTRA_VALUE`, so anything
//! else that is not marked as injected by the OS comes from the local user's mouse.

use super::input_service::mark_local_mouse_active;
use hbb_common::log;
use std::{
    io::Error,
    ptr::null_mut,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};
use winapi::{
    ctypes::c_int,
    shared::minwindef::{LPARAM, LRESULT, WPARAM},
    um::{libloaderapi::GetModuleHandleW, processthreadsapi::GetCurrentThreadId, winuser::*},
};

static HOOK_RUNNING: AtomicBool = AtomicBool::new(false);
static HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);

unsafe extern "system" fn local_mouse_proc(
    code: c_int,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if code == HC_ACTION {
        let ms = &*(l_param as *const MSLLHOOKSTRUCT);
        if ms.dwExtraInfo != enigo::ENIGO_INPUT_EXTRA_VALUE && ms.flags & LLMHF_INJECTED == 0 {
            mark_local_mouse_active();
        }
    }
    CallNextHookEx(null_mut(), code, w_param, l_param)
}

pub fn start() {
    if HOOK_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| unsafe {
        let hook = SetWindowsHookExW(
            WH_MOUSE_LL,
            Some(local_mouse_proc),
            GetModuleHandleW(null_mut()),
            0,
        );
        if hook.is_null() {
            log::error!(
                "Failed to hook local mouse, error: {}",
                Error::last_os_error()
            );
            HOOK_RUNNING.store(false, Ordering::SeqCst);
            return;
        }
        let mut msg: MSG = std::mem::zeroed();
        // Create the thread message queue before publishing the thread id, so `stop` can post to it.
        PeekMessageW(&mut msg, null_mut(), WM_USER, WM_USER, PM_NOREMOVE);
        HOOK_THREAD_ID.store(GetCurrentThreadId(), Ordering::SeqCst);
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if UnhookWindowsHookEx(hook) == 0 {
            log::error!(
                "Failed to unhook local mouse, error: {}",
                Error::last_os_error()
            );
        }
        HOOK_THREAD_ID.store(0, Ordering::SeqCst);
        HOOK_RUNNING.store(false, Ordering::SeqCst);
    });
}

pub fn stop() {
    let thread_id = HOOK_THREAD_ID.load(Ordering::SeqCst);
    if thread_id != 0 {
        unsafe {
            PostThreadMessageW(thread_id, WM_QUIT, 0, 0);
        }
    }
}

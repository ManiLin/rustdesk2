//! Cashdesk: require permanent password to stop service / exit while remote sessions are active.

use hbb_common::config::{self, Config};

#[inline]
pub fn is_cashdesk_build() -> bool {
    config::is_incoming_only()
}

#[inline]
pub fn controlled_session_count() -> usize {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        return crate::ipc::get_controlled_session_count_sync();
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        0
    }
}

#[inline]
pub fn exit_password_required() -> bool {
    is_cashdesk_build() && controlled_session_count() > 0
}

/// Prompt for password when required; returns true if shutdown may proceed.
pub fn confirm_shutdown() -> bool {
    if !exit_password_required() {
        return true;
    }
    loop {
        let Some(pwd) = platform_prompt_exit_password() else {
            return false;
        };
        if Config::matches_permanent_password_plain(&pwd) {
            return true;
        }
        platform_show_exit_password_wrong();
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn platform_prompt_exit_password() -> Option<String> {
    crate::platform::prompt_exit_password()
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn platform_show_exit_password_wrong() {
    crate::platform::show_exit_password_wrong();
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn platform_prompt_exit_password() -> Option<String> {
    None
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn platform_show_exit_password_wrong() {}

//! Build-time defaults for Tatnefturs AD / address book (main crate — not hbb_common submodule).

include!(concat!(env!("OUT_DIR"), "/app_build_defaults.rs"));

use hbb_common::config::{self, keys};

#[inline]
pub fn is_cashdesk_ui_build() -> bool {
    DEFAULT_DESKTOP_UI_FLAVOR_FROM_BUILD.eq_ignore_ascii_case("cashdesk")
}

#[inline]
pub fn ad_address_book_features_enabled() -> bool {
    !is_cashdesk_ui_build() && !config::is_incoming_only()
}

/// Apply build-time api-server and preset address book into DEFAULT_SETTINGS.
pub fn apply_build_server_defaults() {
    if !DEFAULT_API_SERVER_FROM_BUILD.is_empty() {
        config::DEFAULT_SETTINGS.write().unwrap().insert(
            keys::OPTION_API_SERVER.to_owned(),
            DEFAULT_API_SERVER_FROM_BUILD.to_owned(),
        );
    }
    if ad_address_book_features_enabled() && !DEFAULT_PRESET_ADDRESS_BOOK_NAME_FROM_BUILD.is_empty()
    {
        config::DEFAULT_SETTINGS.write().unwrap().insert(
            keys::OPTION_PRESET_ADDRESS_BOOK_NAME.to_owned(),
            DEFAULT_PRESET_ADDRESS_BOOK_NAME_FROM_BUILD.to_owned(),
        );
    }
}

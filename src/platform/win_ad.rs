//! Active Directory helpers for corp.tatnefturs.ru address book auto-registration.

use hbb_common::config::DEFAULT_AD_DOMAIN_FROM_BUILD;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use winapi::shared::minwindef::{FALSE, TRUE};
use winapi::shared::ntdef::NULL;

#[repr(C)]
#[allow(non_camel_case_types)]
enum ExtendedNameFormat {
    NameSamCompatible = 2,
    NameDisplay = 3,
}

#[repr(C)]
#[allow(non_camel_case_types)]
enum ComputerNameFormat {
    ComputerNameDnsDomain = 2,
}

#[link(name = "Secur32")]
extern "system" {
    fn TranslateNameW(
        lpAccountName: *const u16,
        AccountNameFormat: ExtendedNameFormat,
        DesiredNameFormat: ExtendedNameFormat,
        lpTranslatedName: *mut u16,
        nSize: *mut u32,
    ) -> u8;
}

#[link(name = "Kernel32")]
extern "system" {
    fn GetComputerNameExW(
        name_type: ComputerNameFormat,
        lpBuffer: *mut u16,
        nSize: *mut u32,
    ) -> i32;
}

fn target_ad_domain() -> &'static str {
    if DEFAULT_AD_DOMAIN_FROM_BUILD.is_empty() {
        "corp.tatnefturs.ru"
    } else {
        DEFAULT_AD_DOMAIN_FROM_BUILD
    }
}

fn wide_null(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn get_computer_dns_domain() -> String {
    let mut buf = vec![0u16; 256];
    let mut size = buf.len() as u32;
    unsafe {
        if GetComputerNameExW(ComputerNameFormat::ComputerNameDnsDomain, buf.as_mut_ptr(), &mut size)
            == FALSE
        {
            return String::new();
        }
    }
    let len = size.min(buf.len() as u32) as usize;
    String::from_utf16_lossy(&buf[..len])
        .trim_end_matches('\0')
        .to_string()
}

fn domains_equal(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn translate_sam_to_display(sam: &str) -> Option<String> {
    let sam_wide = wide_null(sam);
    let mut size: u32 = 0;
    unsafe {
        TranslateNameW(
            sam_wide.as_ptr(),
            ExtendedNameFormat::NameSamCompatible,
            ExtendedNameFormat::NameDisplay,
            NULL,
            &mut size,
        );
    }
    if size == 0 {
        return None;
    }
    let mut buf = vec![0u16; size as usize];
    unsafe {
        if TranslateNameW(
            sam_wide.as_ptr(),
            ExtendedNameFormat::NameSamCompatible,
            ExtendedNameFormat::NameDisplay,
            buf.as_mut_ptr(),
            &mut size,
        ) != TRUE
        {
            return None;
        }
    }
    let name = String::from_utf16_lossy(&buf[..size as usize])
        .trim_end_matches('\0')
        .trim()
        .to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn sam_account_name(username: &str) -> &str {
    username
        .rsplit('\\')
        .next()
        .unwrap_or(username)
        .rsplit('@')
        .next()
        .unwrap_or(username)
}

fn build_sam_name(username: &str, dns_domain: &str) -> String {
    if username.contains('\\') {
        username.to_owned()
    } else {
        format!("{}\\{}", dns_domain, sam_account_name(username))
    }
}

/// True when this PC is joined to the configured AD domain (e.g. corp.tatnefturs.ru).
pub fn is_target_ad_domain() -> bool {
    let dns = get_computer_dns_domain();
    !dns.is_empty() && domains_equal(&dns, target_ad_domain())
}

/// Display name (FIO) of the user at the active console session, resolved via AD.
pub fn get_active_user_display_name() -> Option<String> {
    if !is_target_ad_domain() {
        return None;
    }
    let username = super::get_active_username();
    if username.is_empty() || username.eq_ignore_ascii_case("SYSTEM") {
        return None;
    }
    let dns_domain = get_computer_dns_domain();
    if dns_domain.is_empty() {
        return None;
    }
    let sam = build_sam_name(&username, &dns_domain);
    translate_sam_to_display(&sam).or_else(|| {
        let plain = sam_account_name(&username).to_string();
        if plain.is_empty() {
            None
        } else {
            Some(plain)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sam_account_name_parses_domain_user() {
        assert_eq!(sam_account_name("CORP\\ivanov"), "ivanov");
        assert_eq!(sam_account_name("ivanov@corp.tatnefturs.ru"), "ivanov");
    }

    #[test]
    fn build_sam_name_adds_domain() {
        assert_eq!(
            build_sam_name("ivanov", "corp.tatnefturs.ru"),
            "corp.tatnefturs.ru\\ivanov"
        );
        assert_eq!(
            build_sam_name("CORP\\ivanov", "corp.tatnefturs.ru"),
            "CORP\\ivanov"
        );
    }
}

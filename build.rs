#[cfg(windows)]
fn build_windows() {
    let file = "src/platform/windows.cc";
    let file2 = "src/platform/windows_delete_test_cert.cc";
    cc::Build::new().file(file).file(file2).compile("windows");
    println!("cargo:rustc-link-lib=WtsApi32");
    println!("cargo:rerun-if-changed={}", file);
    println!("cargo:rerun-if-changed={}", file2);
}

#[cfg(target_os = "macos")]
fn build_mac() {
    let file = "src/platform/macos.mm";
    let mut b = cc::Build::new();
    if let Ok(os_version::OsVersion::MacOS(v)) = os_version::detect() {
        let v = v.version;
        if v.contains("10.14") {
            b.flag("-DNO_InputMonitoringAuthStatus=1");
        }
    }
    b.flag("-std=c++17").file(file).compile("macos");
    println!("cargo:rerun-if-changed={}", file);
}

fn install_android_deps() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    if target_os != "android" {
        return;
    }
    let mut target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    if target_arch == "x86_64" {
        target_arch = "x64".to_owned();
    } else if target_arch == "x86" {
        target_arch = "x86".to_owned();
    } else if target_arch == "aarch64" {
        target_arch = "arm64".to_owned();
    } else {
        target_arch = "arm".to_owned();
    }
    let target = format!("{}-android", target_arch);
    let vcpkg_root = std::env::var("VCPKG_ROOT").unwrap();
    let mut path: std::path::PathBuf = vcpkg_root.into();
    if let Ok(vcpkg_root) = std::env::var("VCPKG_INSTALLED_ROOT") {
        path = vcpkg_root.into();
    } else {
        path.push("installed");
    }
    path.push(target);
    println!(
        "cargo:rustc-link-search={}",
        path.join("lib").to_str().unwrap()
    );
    println!("cargo:rustc-link-lib=ndk_compat");
    println!("cargo:rustc-link-lib=oboe");
    println!("cargo:rustc-link-lib=c++");
    println!("cargo:rustc-link-lib=OpenSLES");
}

fn main() {
    hbb_common::gen_version();
    gen_app_build_defaults();
    install_android_deps();
    #[cfg(all(windows, feature = "inline"))]
    build_manifest();
    #[cfg(windows)]
    build_windows();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    if target_os == "macos" {
        #[cfg(target_os = "macos")]
        build_mac();
        println!("cargo:rustc-link-lib=framework=ApplicationServices");
    }
    gen_sciter_ui_icons();
    println!("cargo:rerun-if-changed=build.rs");
}

fn gen_app_build_defaults() {
    println!("cargo:rerun-if-env-changed=RUSTDESK_DESKTOP_UI_FLAVOR");
    println!("cargo:rerun-if-env-changed=RUSTDESK_API_SERVER");
    println!("cargo:rerun-if-env-changed=RUSTDESK_PRESET_ADDRESS_BOOK_NAME");
    println!("cargo:rerun-if-env-changed=RUSTDESK_AD_DOMAIN");
    println!("cargo:rerun-if-env-changed=RUSTDESK_ASSIGN_API_TOKEN");

    let desktop_ui_flavor = std::env::var("RUSTDESK_DESKTOP_UI_FLAVOR").unwrap_or_default();
    let api_server = std::env::var("RUSTDESK_API_SERVER")
        .unwrap_or_else(|_| "https://rustdeskweb.corp.tatnefturs.ru".to_string());
    let preset_address_book_name = std::env::var("RUSTDESK_PRESET_ADDRESS_BOOK_NAME")
        .unwrap_or_else(|_| "corp.tatnefturs.ru".to_string());
    let ad_domain =
        std::env::var("RUSTDESK_AD_DOMAIN").unwrap_or_else(|_| "corp.tatnefturs.ru".to_string());
    let assign_api_token = if desktop_ui_flavor.eq_ignore_ascii_case("cashdesk") {
        String::new()
    } else {
        std::env::var("RUSTDESK_ASSIGN_API_TOKEN")
            .unwrap_or_else(|_| "68251cb040a223f884fd8bc42d352239".to_string())
    };

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let path = std::path::Path::new(&out_dir).join("app_build_defaults.rs");
    let src = format!(
        "pub const DEFAULT_DESKTOP_UI_FLAVOR_FROM_BUILD: &str = {desktop_ui_flavor:?};\n\
pub const DEFAULT_API_SERVER_FROM_BUILD: &str = {api_server:?};\n\
pub const DEFAULT_PRESET_ADDRESS_BOOK_NAME_FROM_BUILD: &str = {preset_address_book_name:?};\n\
pub const DEFAULT_AD_DOMAIN_FROM_BUILD: &str = {ad_domain:?};\n\
pub const DEFAULT_ASSIGN_API_TOKEN_FROM_BUILD: &str = {assign_api_token:?};\n"
    );
    std::fs::write(path, src).expect("write app_build_defaults.rs");
}

/// Embed `res/icon.png` / `res/mac-icon.png` for Sciter UI (window caption, chatbox, etc.).
fn gen_sciter_ui_icons() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    for (out_name, path) in [
        ("sciter_icon_default.txt", "res/icon.png"),
        ("sciter_icon_macos.txt", "res/mac-icon.png"),
    ] {
        println!("cargo:rerun-if-changed={}", path);
        let data_url = std::fs::read(path)
            .map(|bytes| format!("data:image/png;base64,{}", base64_encode(&bytes)))
            .unwrap_or_default();
        let out_path = std::path::Path::new(&out_dir).join(out_name);
        std::fs::write(out_path, data_url).ok();
    }
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(all(windows, feature = "inline"))]
fn build_manifest() {
    use std::io::Write;
    if std::env::var("PROFILE").unwrap() == "release" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("res/icon.ico")
            .set_language(winapi::um::winnt::MAKELANGID(
                winapi::um::winnt::LANG_ENGLISH,
                winapi::um::winnt::SUBLANG_ENGLISH_US,
            ))
            .set_manifest_file("res/manifest.xml");
        match res.compile() {
            Err(e) => {
                write!(std::io::stderr(), "{}", e).unwrap();
                std::process::exit(1);
            }
            Ok(_) => {}
        }
    }
}

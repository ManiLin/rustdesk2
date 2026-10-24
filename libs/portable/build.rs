fn main() {
    #[cfg(windows)]
    {
        use std::io::Write;
        let app_name =
            std::env::var("RUSTDESK_APP_NAME").unwrap_or_else(|_| "TnursRemoteDesk".to_string());
        let exe_base = app_name.to_ascii_lowercase();
        let mut res = winres::WindowsResource::new();
        res.set_icon("../../res/icon.ico")
            .set_language(winapi::um::winnt::MAKELANGID(
                winapi::um::winnt::LANG_ENGLISH,
                winapi::um::winnt::SUBLANG_ENGLISH_US,
            ))
            .set_manifest_file("../../res/manifest.xml")
            .set("ProductName", &app_name)
            .set("FileDescription", &app_name)
            .set("InternalName", &exe_base)
            .set("OriginalFilename", &format!("{}.exe", exe_base));
        set_numeric_version(&mut res);
        match res.compile() {
            Err(e) => {
                write!(std::io::stderr(), "{}", e).unwrap();
                std::process::exit(1);
            }
            Ok(_) => {}
        }
    }
}

/// `1.4.9-8` -> (1, 4, 9, 8): numeric a.b.c.d version for the exe resources,
/// deployment tools compare FileVersion/ProductVersion as numbers.
#[cfg(windows)]
fn numeric_version(version: &str) -> [u16; 4] {
    let (base, revision) = version.split_once('-').unwrap_or((version, "0"));
    let num = |s: &str| -> u16 {
        s.chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .unwrap_or(0)
    };
    let mut parts = base.split('.').map(num);
    [
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        num(revision),
    ]
}

#[cfg(windows)]
fn set_numeric_version(res: &mut winres::WindowsResource) {
    let v = numeric_version(&std::env::var("CARGO_PKG_VERSION").unwrap_or_default());
    let packed = ((v[0] as u64) << 48) | ((v[1] as u64) << 32) | ((v[2] as u64) << 16) | v[3] as u64;
    let text = format!("{}.{}.{}.{}", v[0], v[1], v[2], v[3]);
    res.set_version_info(winres::VersionInfo::FILEVERSION, packed)
        .set_version_info(winres::VersionInfo::PRODUCTVERSION, packed)
        .set("FileVersion", &text)
        .set("ProductVersion", &text);
}

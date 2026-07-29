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
        match res.compile() {
            Err(e) => {
                write!(std::io::stderr(), "{}", e).unwrap();
                std::process::exit(1);
            }
            Ok(_) => {}
        }
    }
}

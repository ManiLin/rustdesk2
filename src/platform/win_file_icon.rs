//! Windows shell icons for the file manager (PNG for Flutter).
//!
//! Files get their real icon: by path for local executables / shortcuts / icon
//! files, otherwise by extension (remote entries only have a name). Folders get
//! an icon only when it differs from the default folder icon (Desktop,
//! Downloads, folders with `desktop.ini`, ...); otherwise the result is empty
//! and the UI keeps its own folder icon.

use std::{
    collections::HashMap,
    ffi::OsStr,
    io::Cursor,
    mem::{size_of, zeroed},
    os::windows::ffi::OsStrExt,
    path::Path,
    ptr::null_mut,
    sync::Mutex,
};
use winapi::{
    shared::{minwindef::DWORD, windef::HICON},
    um::{
        combaseapi::CoInitializeEx,
        objbase::COINIT_APARTMENTTHREADED,
        shellapi::{
            SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_SYSICONINDEX,
            SHGFI_USEFILEATTRIBUTES,
        },
        wingdi::{
            DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
            DIB_RGB_COLORS,
        },
        winnt::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL},
        winuser::{DestroyIcon, GetDC, GetIconInfo, ReleaseDC, ICONINFO},
    },
};

/// Extensions whose icon is specific to the file itself, not to its type.
const PER_FILE_EXTENSIONS: [&str; 6] = ["exe", "ico", "lnk", "url", "cur", "scr"];

/// User profile folders with a special shell icon; remote ones are matched by
/// name and shown with the icon of the local folder of the same name.
const SPECIAL_FOLDERS: [&str; 14] = [
    "Desktop",
    "Documents",
    "Downloads",
    "Music",
    "Pictures",
    "Videos",
    "Favorites",
    "Links",
    "Contacts",
    "Saved Games",
    "Searches",
    "3D Objects",
    "OneDrive",
    "AppData",
];

lazy_static::lazy_static! {
    static ref CACHE: Mutex<HashMap<String, Vec<u8>>> = Default::default();
    static ref DEFAULT_FOLDER_INDEX: i32 = unsafe { default_folder_index() };
}

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// Shell icon + its system image list index.
unsafe fn shell_icon(path: &str, attributes: DWORD, by_attributes: bool) -> Option<(HICON, i32)> {
    let mut info: SHFILEINFOW = zeroed();
    let mut flags = SHGFI_ICON | SHGFI_LARGEICON | SHGFI_SYSICONINDEX;
    if by_attributes {
        flags |= SHGFI_USEFILEATTRIBUTES;
    }
    let path = wide(path);
    let res = SHGetFileInfoW(
        path.as_ptr(),
        attributes,
        &mut info,
        size_of::<SHFILEINFOW>() as u32,
        flags,
    );
    if res == 0 || info.hIcon.is_null() {
        return None;
    }
    Some((info.hIcon, info.iIcon))
}

unsafe fn default_folder_index() -> i32 {
    match shell_icon("folder", FILE_ATTRIBUTE_DIRECTORY, true) {
        Some((icon, index)) => {
            DestroyIcon(icon);
            index
        }
        None => -1,
    }
}

/// Convert an icon to a PNG with alpha.
unsafe fn icon_to_png(icon: HICON) -> Option<Vec<u8>> {
    let mut info: ICONINFO = zeroed();
    if GetIconInfo(icon, &mut info) == 0 {
        return None;
    }
    let result = (|| {
        if info.hbmColor.is_null() {
            return None;
        }
        let mut bmp: BITMAP = zeroed();
        if GetObjectW(
            info.hbmColor as _,
            size_of::<BITMAP>() as i32,
            &mut bmp as *mut _ as _,
        ) == 0
        {
            return None;
        }
        let (w, h) = (bmp.bmWidth, bmp.bmHeight);
        if w <= 0 || h <= 0 || w > 256 || h > 256 {
            return None;
        }
        let mut bi: BITMAPINFO = zeroed();
        bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = w;
        bi.bmiHeader.biHeight = -h; // top-down
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        bi.bmiHeader.biCompression = BI_RGB;
        let dc = GetDC(null_mut());
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        let lines = GetDIBits(
            dc,
            info.hbmColor,
            0,
            h as u32,
            pixels.as_mut_ptr() as _,
            &mut bi,
            DIB_RGB_COLORS,
        );
        // Old-style icons have no alpha channel: take transparency from the mask.
        if lines > 0 && pixels.chunks(4).all(|p| p[3] == 0) && !info.hbmMask.is_null() {
            let mut mask = vec![0u8; (w * h * 4) as usize];
            if GetDIBits(
                dc,
                info.hbmMask,
                0,
                h as u32,
                mask.as_mut_ptr() as _,
                &mut bi,
                DIB_RGB_COLORS,
            ) > 0
            {
                for (p, m) in pixels.chunks_mut(4).zip(mask.chunks(4)) {
                    p[3] = if m[0] == 0 { 255 } else { 0 };
                }
            }
        }
        ReleaseDC(null_mut(), dc);
        if lines == 0 {
            return None;
        }
        for p in pixels.chunks_mut(4) {
            p.swap(0, 2); // BGRA -> RGBA
        }
        let img = image::RgbaImage::from_raw(w as u32, h as u32, pixels)?;
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut out, image::ImageOutputFormat::Png)
            .ok()?;
        Some(out.into_inner())
    })();
    if !info.hbmColor.is_null() {
        DeleteObject(info.hbmColor as _);
    }
    if !info.hbmMask.is_null() {
        DeleteObject(info.hbmMask as _);
    }
    result
}

/// PNG of the shell icon, skipping the default folder icon (empty result).
unsafe fn png_of(path: &str, attributes: DWORD, by_attributes: bool, is_dir: bool) -> Vec<u8> {
    let Some((icon, index)) = shell_icon(path, attributes, by_attributes) else {
        return Vec::new();
    };
    let png = if is_dir && index == *DEFAULT_FOLDER_INDEX {
        None
    } else {
        icon_to_png(icon)
    };
    DestroyIcon(icon);
    png.unwrap_or_default()
}

fn extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// PNG icon for a file manager entry; empty means "use the default icon".
///
/// `local`: `path` is a full local path. Otherwise only its last component
/// (the remote file name) is used.
pub fn file_icon_png(path: &str, is_dir: bool, local: bool) -> Vec<u8> {
    let name = path
        .rsplit(|c| c == '\\' || c == '/')
        .next()
        .unwrap_or(path)
        .to_owned();
    let ext = extension(&name);
    let key = if is_dir {
        if local {
            format!("dir:{}", path.to_lowercase())
        } else {
            format!("rdir:{}", name.to_lowercase())
        }
    } else if local && PER_FILE_EXTENSIONS.contains(&ext.as_str()) {
        format!("file:{}", path.to_lowercase())
    } else {
        format!("ext:{}", ext)
    };
    if let Some(png) = CACHE.lock().unwrap().get(&key) {
        return png.clone();
    }

    unsafe {
        // The shell needs COM on the calling thread; repeated calls are harmless.
        CoInitializeEx(null_mut(), COINIT_APARTMENTTHREADED);
    }
    let png = unsafe {
        if is_dir {
            if local {
                png_of(path, 0, false, true)
            } else if let Some(folder) = SPECIAL_FOLDERS
                .iter()
                .find(|f| f.eq_ignore_ascii_case(&name))
            {
                match std::env::var("USERPROFILE") {
                    Ok(profile) => {
                        let local_folder = Path::new(&profile).join(folder);
                        if local_folder.is_dir() {
                            png_of(&local_folder.to_string_lossy(), 0, false, true)
                        } else {
                            Vec::new()
                        }
                    }
                    Err(_) => Vec::new(),
                }
            } else {
                Vec::new()
            }
        } else if key.starts_with("file:") {
            png_of(path, 0, false, false)
        } else {
            // Icon of the file type; the file itself does not need to exist.
            let probe = if ext.is_empty() {
                "file".to_owned()
            } else {
                format!("file.{}", ext)
            };
            png_of(&probe, FILE_ATTRIBUTE_NORMAL, true, false)
        }
    };
    CACHE.lock().unwrap().insert(key, png.clone());
    png
}

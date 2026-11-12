//! A program's icon, as plain RGBA pixels.

use std::path::Path;

use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDIBits, HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO, PrivateExtractIconsW};

/// An icon's pixels, top row first, four bytes (R, G, B, A) per pixel.
pub struct IconImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The program's main icon at `size` × `size` pixels, if it has one.
pub fn program_icon(path: &Path, size: u32) -> Option<IconImage> {
    let mut name = [0u16; 260];
    let wide: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().collect();
    if wide.len() >= name.len() {
        return None;
    }
    name[..wide.len()].copy_from_slice(&wide);

    let mut icons = [HICON::default()];
    // SAFETY: `name` is NUL-terminated (zero-filled) and `icons` has room for one.
    let found = unsafe { PrivateExtractIconsW(&name, 0, size as i32, size as i32, Some(&mut icons), None, 0) };
    if found == 0 || found == u32::MAX || icons[0].is_invalid() {
        return None;
    }
    let icon = icons[0];
    // SAFETY: the icon is ours and destroyed after reading.
    let image = unsafe { pixels(icon) };
    // SAFETY: as above.
    unsafe {
        let _ = DestroyIcon(icon);
    }
    image
}

/// Reads an icon's colour bitmap. Old icons without an alpha channel get
/// their transparency from the mask.
unsafe fn pixels(icon: HICON) -> Option<IconImage> {
    // SAFETY: GDI objects are released before returning.
    unsafe {
        let mut info = ICONINFO::default();
        GetIconInfo(icon, &mut info).ok()?;
        let dc = CreateCompatibleDC(None);
        let result = (|| {
            if info.hbmColor.is_invalid() {
                return None;
            }
            let (width, height, mut bgra) = read_bitmap(dc, info.hbmColor)?;
            let has_alpha = bgra.as_chunks::<4>().0.iter().any(|px| px[3] != 0);
            if !has_alpha && !info.hbmMask.is_invalid() {
                if let Some((_, _, mask)) = read_bitmap(dc, info.hbmMask) {
                    for (px, m) in bgra.as_chunks_mut::<4>().0.iter_mut().zip(mask.as_chunks::<4>().0) {
                        // In the mask, black means "part of the icon".
                        px[3] = if m[0] == 0 { 255 } else { 0 };
                    }
                } else {
                    bgra.as_chunks_mut::<4>().0.iter_mut().for_each(|px| px[3] = 255);
                }
            }
            for px in bgra.as_chunks_mut::<4>().0 {
                px.swap(0, 2);
            }
            Some(IconImage { width, height, rgba: bgra })
        })();
        let _ = DeleteDC(dc);
        if !info.hbmColor.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
        }
        if !info.hbmMask.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
        }
        result
    }
}

/// A bitmap as 32-bit BGRA, top row first.
unsafe fn read_bitmap(
    dc: windows::Win32::Graphics::Gdi::HDC,
    bitmap: windows::Win32::Graphics::Gdi::HBITMAP,
) -> Option<(u32, u32, Vec<u8>)> {
    let mut header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, ..Default::default() },
        ..Default::default()
    };
    // SAFETY: a first call with no buffer only fills in the size.
    unsafe {
        if GetDIBits(dc, bitmap, 0, 0, None, &mut header, DIB_RGB_COLORS) == 0 {
            return None;
        }
    }
    let width = header.bmiHeader.biWidth.unsigned_abs();
    let height = header.bmiHeader.biHeight.unsigned_abs();
    if width == 0 || height == 0 || width > 512 || height > 512 {
        return None;
    }
    header.bmiHeader.biBitCount = 32;
    header.bmiHeader.biCompression = BI_RGB.0;
    // A negative height asks for the top row first.
    header.bmiHeader.biHeight = -(height as i32);
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    // SAFETY: `pixels` holds width × height 32-bit pixels.
    let lines = unsafe {
        GetDIBits(dc, bitmap, 0, height, Some(pixels.as_mut_ptr().cast()), &mut header, DIB_RGB_COLORS)
    };
    (lines as u32 == height).then_some((width, height, pixels))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_windows_program_icon() {
        let windows = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let image = program_icon(&Path::new(&windows).join(r"System32\notepad.exe"), 32).expect("notepad has an icon");
        assert_eq!((image.width, image.height), (32, 32));
        assert_eq!(image.rgba.len(), 32 * 32 * 4);
        // Some pixels are see-through and some are not.
        let pixels = image.rgba.as_chunks::<4>().0;
        assert!(pixels.iter().any(|px| px[3] == 0));
        assert!(pixels.iter().any(|px| px[3] == 255));
    }
}

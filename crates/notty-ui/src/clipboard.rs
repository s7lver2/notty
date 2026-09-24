use windows::Win32::Foundation::HWND;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GHND, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::core::Result;

/// Pone `text` en el portapapeles como CF_UNICODETEXT (terminado en NUL, como espera Windows).
pub fn set_clipboard_text(hwnd: HWND, text: &str) -> Result<()> {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = wide.len() * std::mem::size_of::<u16>();

    unsafe {
        OpenClipboard(Some(hwnd))?;
        let result = (|| -> Result<()> {
            EmptyClipboard()?;
            let handle = GlobalAlloc(GHND, bytes)?;
            let ptr = GlobalLock(handle) as *mut u16;
            if ptr.is_null() {
                return Err(windows::core::Error::from_thread());
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
            let _ = GlobalUnlock(handle);
            SetClipboardData(CF_UNICODETEXT.0 as u32, Some(windows::Win32::Foundation::HANDLE(handle.0)))?;
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
}

/// Lee el portapapeles como texto. Si no hay texto Unicode, devuelve cadena vacía.
pub fn get_clipboard_text(hwnd: HWND) -> Result<String> {
    unsafe {
        OpenClipboard(Some(hwnd))?;
        let result = (|| -> Result<String> {
            let Ok(handle) = GetClipboardData(CF_UNICODETEXT.0 as u32) else {
                return Ok(String::new());
            };
            let ptr = GlobalLock(windows::Win32::Foundation::HGLOBAL(handle.0)) as *const u16;
            if ptr.is_null() {
                return Ok(String::new());
            }
            let len_bytes = GlobalSize(windows::Win32::Foundation::HGLOBAL(handle.0));
            let len_u16 = len_bytes / std::mem::size_of::<u16>();
            let slice = std::slice::from_raw_parts(ptr, len_u16);
            let end = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
            let text = String::from_utf16_lossy(&slice[..end]);
            let _ = GlobalUnlock(windows::Win32::Foundation::HGLOBAL(handle.0));
            Ok(text)
        })();
        let _ = CloseClipboard();
        result
    }
}

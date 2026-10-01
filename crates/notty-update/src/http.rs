//! Llamadas de red reales via WinHTTP: sin esto, el resto del crate es puro y no
//! toca la red. No es testeable con `cargo test` (I/O real contra api.github.com);
//! se verifica a mano (ver el plan, Task 3, Step 3).
//!
//! Nota de implementación: a diferencia de otras familias de la crate `windows`,
//! los bindings de WinHTTP en la 0.62.2 son "crudos" — devuelven `*mut c_void`
//! (nulo si falla) en vez de un `HANDLE`/`Result` envuelto, y varias funciones
//! toman punteros crudos en vez de `Option<&mut T>`. Por eso este módulo comprueba
//! nulos a mano en vez de `.is_invalid()`/`?`.

use windows::Win32::Networking::WinHttp::*;
use windows::core::HSTRING;

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("sin red o error HTTP: {0}")]
    Network(String),
    #[error(transparent)]
    Release(#[from] crate::release::ReleaseError),
}

pub fn latest_release(owner_repo: &str, user_agent: &str) -> Result<crate::Release, HttpError> {
    let json = get("api.github.com", &format!("/repos/{owner_repo}/releases/latest"), user_agent, "application/vnd.github+json")?;
    Ok(crate::parse_release(&json)?)
}

/// La última release de `owner_repo` con el instalador `setup` (y su `.sig`).
pub fn latest_release_with(owner_repo: &str, setup: &str, user_agent: &str) -> Result<crate::Release, HttpError> {
    let json = get("api.github.com", &format!("/repos/{owner_repo}/releases/latest"), user_agent, "application/vnd.github+json")?;
    Ok(crate::parse_release_with(&json, setup)?)
}

struct Handles {
    session: *mut core::ffi::c_void,
    connect: *mut core::ffi::c_void,
    request: *mut core::ffi::c_void,
}

impl Drop for Handles {
    fn drop(&mut self) {
        unsafe {
            if !self.request.is_null() {
                let _ = WinHttpCloseHandle(self.request);
            }
            if !self.connect.is_null() {
                let _ = WinHttpCloseHandle(self.connect);
            }
            if !self.session.is_null() {
                let _ = WinHttpCloseHandle(self.session);
            }
        }
    }
}

fn open_request(host: &str, path: &str, user_agent: &str) -> Result<Handles, HttpError> {
    unsafe {
        let session = WinHttpOpen(&HSTRING::from(user_agent), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, windows::core::PCWSTR::null(), windows::core::PCWSTR::null(), 0);
        if session.is_null() {
            return Err(HttpError::Network("WinHttpOpen falló".to_string()));
        }
        let connect = WinHttpConnect(session, &HSTRING::from(host), INTERNET_DEFAULT_HTTPS_PORT, 0);
        if connect.is_null() {
            let _ = WinHttpCloseHandle(session);
            return Err(HttpError::Network("WinHttpConnect falló".to_string()));
        }
        let request = WinHttpOpenRequest(connect, &HSTRING::from("GET"), &HSTRING::from(path), windows::core::PCWSTR::null(), windows::core::PCWSTR::null(), std::ptr::null(), WINHTTP_FLAG_SECURE);
        if request.is_null() {
            let _ = WinHttpCloseHandle(connect);
            let _ = WinHttpCloseHandle(session);
            return Err(HttpError::Network("WinHttpOpenRequest falló".to_string()));
        }
        Ok(Handles { session, connect, request })
    }
}

fn get(host: &str, path: &str, user_agent: &str, accept: &str) -> Result<String, HttpError> {
    let handles = open_request(host, path, user_agent)?;
    unsafe {
        let headers: Vec<u16> = format!("Accept: {accept}\r\n").encode_utf16().collect();
        WinHttpAddRequestHeaders(handles.request, &headers, WINHTTP_ADDREQ_FLAG_ADD).map_err(|e| HttpError::Network(e.to_string()))?;

        WinHttpSendRequest(handles.request, None, None, 0, 0, 0).map_err(|e| HttpError::Network(e.to_string()))?;
        WinHttpReceiveResponse(handles.request, std::ptr::null_mut()).map_err(|e| HttpError::Network(e.to_string()))?;

        let mut buf = Vec::new();
        loop {
            let mut available: u32 = 0;
            WinHttpQueryDataAvailable(handles.request, &mut available).map_err(|e| HttpError::Network(e.to_string()))?;
            if available == 0 {
                break;
            }
            let mut chunk = vec![0u8; available as usize];
            let mut read: u32 = 0;
            WinHttpReadData(handles.request, chunk.as_mut_ptr() as *mut core::ffi::c_void, chunk.len() as u32, &mut read).map_err(|e| HttpError::Network(e.to_string()))?;
            chunk.truncate(read as usize);
            buf.extend_from_slice(&chunk);
        }
        String::from_utf8(buf).map_err(|e| HttpError::Network(e.to_string()))
    }
}

/// Descarga `url` (HTTPS simple, como los assets de GitHub Releases) a `dest`,
/// informando `on_progress(bytes_leidos, content_length)` en cada trozo leído.
/// Si algo falla a mitad de la descarga, borra el archivo parcial antes de volver.
pub fn download(url: &str, dest: &std::path::Path, mut on_progress: impl FnMut(u64, u64)) -> Result<(), HttpError> {
    let (host, path) = split_https_url(url).ok_or_else(|| HttpError::Network(format!("URL no soportada: {url}")))?;
    let result = (|| -> Result<(), HttpError> {
        let handles = open_request(&host, &path, "notty-update")?;
        unsafe {
            WinHttpSendRequest(handles.request, None, None, 0, 0, 0).map_err(|e| HttpError::Network(e.to_string()))?;
            WinHttpReceiveResponse(handles.request, std::ptr::null_mut()).map_err(|e| HttpError::Network(e.to_string()))?;

            let content_length = query_content_length(handles.request).unwrap_or(0);

            let mut file = std::fs::File::create(dest).map_err(|e| HttpError::Network(e.to_string()))?;
            let mut total: u64 = 0;
            loop {
                let mut available: u32 = 0;
                WinHttpQueryDataAvailable(handles.request, &mut available).map_err(|e| HttpError::Network(e.to_string()))?;
                if available == 0 {
                    break;
                }
                let mut chunk = vec![0u8; available as usize];
                let mut read: u32 = 0;
                WinHttpReadData(handles.request, chunk.as_mut_ptr() as *mut core::ffi::c_void, chunk.len() as u32, &mut read).map_err(|e| HttpError::Network(e.to_string()))?;
                chunk.truncate(read as usize);
                std::io::Write::write_all(&mut file, &chunk).map_err(|e| HttpError::Network(e.to_string()))?;
                total += chunk.len() as u64;
                on_progress(total, content_length);
            }
            Ok(())
        }
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(dest);
    }
    result
}

/// Parte una URL HTTPS simple (como las de los assets de GitHub Releases) en
/// (host, ruta con query). Evita añadir una dependencia nueva solo para esto.
fn split_https_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("https://")?;
    match rest.split_once('/') {
        Some((host, path)) => Some((host.to_string(), format!("/{path}"))),
        None => Some((rest.to_string(), "/".to_string())),
    }
}

fn query_content_length(request: *mut core::ffi::c_void) -> Option<u64> {
    unsafe {
        let mut value: u32 = 0;
        let mut len: u32 = std::mem::size_of::<u32>() as u32;
        let mut index: u32 = 0;
        WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_CONTENT_LENGTH | WINHTTP_QUERY_FLAG_NUMBER,
            windows::core::PCWSTR::null(),
            Some(&mut value as *mut u32 as *mut core::ffi::c_void),
            &mut len,
            &mut index,
        )
        .ok()?;
        Some(value as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_https_url_extracts_host_and_path() {
        assert_eq!(split_https_url("https://example.com/notty-setup.exe"), Some(("example.com".to_string(), "/notty-setup.exe".to_string())));
    }

    #[test]
    fn split_https_url_defaults_to_root_path() {
        assert_eq!(split_https_url("https://example.com"), Some(("example.com".to_string(), "/".to_string())));
    }

    #[test]
    fn split_https_url_rejects_non_https() {
        assert_eq!(split_https_url("http://example.com/x"), None);
    }
}

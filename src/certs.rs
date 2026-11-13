//! A CA bundle for yt-dlp's browser-imitating connections (used for
//! Instagram, sometimes TikTok). Those bring their own certificate list and
//! ignore Windows', so they fail behind antivirus HTTPS scanning (Avast, AVG,
//! Kaspersky …). Fix: Mozilla's list (cacert.pem, fetched with the tools)
//! plus every root certificate Windows trusts, written to one PEM file.

use std::ffi::c_void;
use std::path::PathBuf;

use crate::tools;

#[repr(C)]
struct CertContext {
    encoding: u32,
    encoded: *const u8,
    len: u32,
    info: *const c_void,
    store: *const c_void,
}

#[link(name = "crypt32")]
extern "system" {
    fn CertOpenSystemStoreW(provider: usize, name: *const u16) -> *mut c_void;
    fn CertEnumCertificatesInStore(store: *mut c_void, prev: *const CertContext) -> *const CertContext;
    fn CertCloseStore(store: *mut c_void, flags: u32) -> i32;
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len() * 4 / 3 + 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Every root certificate in the user's Windows "ROOT" store, as PEM.
fn windows_roots() -> String {
    let mut pem = String::new();
    let name: Vec<u16> = "ROOT".encode_utf16().chain(Some(0)).collect();
    unsafe {
        let store = CertOpenSystemStoreW(0, name.as_ptr());
        if store.is_null() {
            return pem;
        }
        let mut cert: *const CertContext = std::ptr::null();
        loop {
            cert = CertEnumCertificatesInStore(store, cert);
            if cert.is_null() {
                break;
            }
            let c = &*cert;
            let der = std::slice::from_raw_parts(c.encoded, c.len as usize);
            pem.push_str("-----BEGIN CERTIFICATE-----\n");
            for line in base64(der).as_bytes().chunks(64) {
                pem.push_str(std::str::from_utf8(line).unwrap_or_default());
                pem.push('\n');
            }
            pem.push_str("-----END CERTIFICATE-----\n");
        }
        CertCloseStore(store, 0);
    }
    pem
}

/// Writes Mozilla's list + Windows' roots to one file and returns its path.
pub fn bundle() -> Option<PathBuf> {
    let mozilla = std::fs::read_to_string(tools::bin().join("cacert.pem")).unwrap_or_default();
    let windows = windows_roots();
    if mozilla.is_empty() && windows.is_empty() {
        return None;
    }
    let path = tools::bin().join("ca-bundle.pem");
    std::fs::write(&path, mozilla + "\n" + &windows).ok()?;
    Some(path)
}

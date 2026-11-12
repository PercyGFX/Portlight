//! Who signed a program: its Authenticode signature, embedded in the file or
//! listed in one of Windows' security catalogs (how most Windows files are
//! signed).
//!
//! Checked without any network access: revocation lists are not fetched, so
//! nothing about the programs on this PC leaves it. That also means a
//! certificate revoked after signing still counts as signed here.

use std::fs::File;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use portlight_core::Signature;
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Security::Cryptography::Catalog::{
    CATALOG_INFO, CryptCATAdminAcquireContext2, CryptCATAdminCalcHashFromFileHandle2, CryptCATAdminEnumCatalogFromHash,
    CryptCATAdminReleaseCatalogContext, CryptCATAdminReleaseContext, CryptCATCatalogInfoFromContext,
};
use windows::Win32::Security::Cryptography::{CERT_NAME_SIMPLE_DISPLAY_TYPE, CertGetNameStringW};
use windows::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_CATALOG_INFO, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
    WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_CATALOG, WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE,
    WTD_STATEACTION_VERIFY, WTD_UI_NONE, WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust,
};
use windows::core::{PCWSTR, w};

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Checks a program's signature. Takes from a few milliseconds to a few
/// hundred, so call it off the UI thread.
pub fn check(path: &Path) -> Signature {
    let Ok(file) = File::open(path) else { return Signature::Unknown };
    let wide_path = wide(&path.to_string_lossy());

    if let Some(publisher) = verify_embedded(&wide_path, &file) {
        return Signature::Signed { publisher };
    }
    if let Some(publisher) = verify_catalog(&wide_path, &file) {
        return Signature::Signed { publisher };
    }
    Signature::Unsigned
}

/// A signature inside the file itself.
fn verify_embedded(path: &[u16], file: &File) -> Option<String> {
    let mut info = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(path.as_ptr()),
        hFile: HANDLE(file.as_raw_handle()),
        pgKnownSubject: std::ptr::null_mut(),
    };
    let mut data = trust_data(WTD_CHOICE_FILE, WINTRUST_DATA_0 { pFile: &mut info });
    // SAFETY: `info` and `path` outlive both calls; the state is closed in `verify`.
    unsafe { verify(&mut data) }
}

/// A signature in a Windows catalog that lists the file's hash.
fn verify_catalog(path: &[u16], file: &File) -> Option<String> {
    // Newer catalogs use SHA-256, older ones SHA-1.
    for algorithm in [w!("SHA256"), w!("SHA1")] {
        // SAFETY: every handle acquired here is released before returning.
        unsafe {
            let mut admin = 0isize;
            if CryptCATAdminAcquireContext2(&mut admin, None, algorithm, None, None).is_err() {
                continue;
            }
            let found = (|| {
                let handle = HANDLE(file.as_raw_handle());
                let mut size = 0u32;
                let _ = CryptCATAdminCalcHashFromFileHandle2(admin, handle, &mut size, None, None);
                if size == 0 || size > 64 {
                    return None;
                }
                let mut hash = vec![0u8; size as usize];
                CryptCATAdminCalcHashFromFileHandle2(admin, handle, &mut size, Some(hash.as_mut_ptr()), None).ok()?;

                let catalog = CryptCATAdminEnumCatalogFromHash(admin, &hash, None, None);
                if catalog == 0 {
                    return None;
                }
                let mut catalog_info = CATALOG_INFO { cbStruct: std::mem::size_of::<CATALOG_INFO>() as u32, ..Default::default() };
                let read = CryptCATCatalogInfoFromContext(catalog, &mut catalog_info, 0);
                let result = read.ok().and_then(|_| {
                    let tag = wide(&hash.iter().map(|b| format!("{b:02X}")).collect::<String>());
                    let mut info = WINTRUST_CATALOG_INFO {
                        cbStruct: std::mem::size_of::<WINTRUST_CATALOG_INFO>() as u32,
                        pcwszCatalogFilePath: PCWSTR(catalog_info.wszCatalogFile.as_ptr()),
                        pcwszMemberTag: PCWSTR(tag.as_ptr()),
                        pcwszMemberFilePath: PCWSTR(path.as_ptr()),
                        hMemberFile: handle,
                        pbCalculatedFileHash: hash.as_mut_ptr(),
                        cbCalculatedFileHash: size,
                        hCatAdmin: admin,
                        ..Default::default()
                    };
                    let mut data = trust_data(WTD_CHOICE_CATALOG, WINTRUST_DATA_0 { pCatalog: &mut info });
                    verify(&mut data)
                });
                let _ = CryptCATAdminReleaseCatalogContext(admin, catalog, 0);
                result
            })();
            let _ = CryptCATAdminReleaseContext(admin, 0);
            if found.is_some() {
                return found;
            }
        }
    }
    None
}

fn trust_data(choice: windows::Win32::Security::WinTrust::WINTRUST_DATA_UNION_CHOICE, subject: WINTRUST_DATA_0) -> WINTRUST_DATA {
    WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: choice,
        Anonymous: subject,
        dwStateAction: WTD_STATEACTION_VERIFY,
        // Never go online to check certificates.
        dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL,
        ..Default::default()
    }
}

/// Runs the check and, when the signature is valid, reads the signer's name.
///
/// # Safety
/// Everything `data` points to must stay alive for the call.
unsafe fn verify(data: &mut WINTRUST_DATA) -> Option<String> {
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: `data` is filled in; the state is closed below whatever the outcome.
    unsafe {
        let status = WinVerifyTrust(HWND(std::ptr::null_mut()), &mut action, data as *mut WINTRUST_DATA as *mut _);
        let publisher = if status == 0 { signer_name(data.hWVTStateData) } else { None };
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        WinVerifyTrust(HWND(std::ptr::null_mut()), &mut action, data as *mut WINTRUST_DATA as *mut _);
        publisher
    }
}

/// The display name on the signing certificate, e.g. "Mozilla Corporation".
unsafe fn signer_name(state: HANDLE) -> Option<String> {
    // SAFETY: `state` came from a successful WinVerifyTrust and is still open.
    unsafe {
        let provider = WTHelperProvDataFromStateData(state);
        if provider.is_null() {
            return None;
        }
        let signer = WTHelperGetProvSignerFromChain(provider, 0, false, 0).as_ref()?;
        if signer.csCertChain == 0 || signer.pasCertChain.is_null() {
            return None;
        }
        let cert = (*signer.pasCertChain).pCert;
        if cert.is_null() {
            return None;
        }
        let mut name = [0u16; 256];
        let len = CertGetNameStringW(cert, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, Some(&mut name));
        let text = String::from_utf16_lossy(&name[..(len as usize).saturating_sub(1).min(name.len())]);
        let text = text.trim();
        (!text.is_empty()).then(|| text.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_programs_are_signed_by_microsoft() {
        let windows = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        // notepad.exe is catalog-signed on some versions and embedded-signed on others.
        for program in [r"System32\notepad.exe", r"System32\svchost.exe"] {
            match check(&Path::new(&windows).join(program)) {
                Signature::Signed { publisher } => assert!(publisher.contains("Microsoft"), "{program}: {publisher}"),
                other => panic!("{program}: {other:?}"),
            }
        }
    }

    #[test]
    fn a_plain_file_is_unsigned_and_a_missing_one_unknown() {
        let dir = std::env::temp_dir().join(format!("portlight-sig-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("plain.exe");
        std::fs::write(&file, b"MZ not really a program").unwrap();
        assert_eq!(check(&file), Signature::Unsigned);
        assert_eq!(check(&dir.join("missing.exe")), Signature::Unknown);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

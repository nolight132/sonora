//! The Rust side of the C++ host in `shim/`, compiled by the build script under the `cdm`
//! feature. It drives the system Widevine module through its official interface to build a
//! license challenge, take the license back and decrypt CENC samples, and reads no key out of
//! it.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::Path;

use anyhow::{Result, bail};

unsafe extern "C" {
    fn ch_open(path: *const c_char) -> c_int;
    fn ch_load_error() -> *const c_char;
    fn ch_challenge(init_data: *const u8, len: u32, out: *mut *mut u8, out_len: *mut u32) -> c_int;
    fn ch_update(license: *const u8, len: u32) -> c_int;
    fn ch_decrypt(
        data: *const u8,
        data_size: u32,
        key_id: *const u8,
        key_id_size: u32,
        iv: *const u8,
        iv_size: u32,
        subs: *const u32,
        num_subs: u32,
        out: *mut *mut u8,
        out_len: *mut u32,
    ) -> c_int;
    fn ch_free(p: *mut u8);
}

/// Copies a buffer the shim handed out and frees it.
unsafe fn take(out: *mut u8, len: u32) -> Vec<u8> {
    if len == 0 {
        unsafe { ch_free(out) };
        return Vec::new();
    }
    unsafe {
        let copied = std::slice::from_raw_parts(out, len as usize).to_vec();
        ch_free(out);
        copied
    }
}

/// A handle to the process Widevine module.
///
/// The native host is a single global instance, so every `Shim` drives the same module and
/// opening again re-initializes it. Only the host process in `crate::host` opens it, once.
pub struct Shim {
    _priv: (),
}

impl Shim {
    /// Loads and initializes the module at `path`, which is read where it is and never copied.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let Some(text) = path.to_str() else {
            bail!("the module path is not utf-8: {}", path.display());
        };
        let path = CString::new(text)?;
        match unsafe { ch_open(path.as_ptr()) } {
            0 => Ok(Shim { _priv: () }),
            1 => {
                let reason = unsafe { CStr::from_ptr(ch_load_error()) };
                bail!("cannot load the module: {}", reason.to_string_lossy())
            }
            2 => bail!("the module exports no cdm entry points"),
            3 => bail!("the module gave no cdm instance"),
            code => bail!("the cdm did not initialize (code {code})"),
        }
    }

    /// The license challenge for a CENC `pssh` box, as the device's Widevine service accepts it.
    pub fn challenge(&self, pssh: &[u8]) -> Result<Vec<u8>> {
        let mut out = std::ptr::null_mut();
        let mut len = 0u32;
        match unsafe { ch_challenge(pssh.as_ptr(), pssh.len() as u32, &mut out, &mut len) } {
            0 => Ok(unsafe { take(out, len) }),
            code => bail!("ch_challenge failed (code {code})"),
        }
    }

    /// Hands the license response to the module, which loads the content keys it carries.
    pub fn update(&self, license: &[u8]) -> Result<()> {
        match unsafe { ch_update(license.as_ptr(), license.len() as u32) } {
            0 => Ok(()),
            code => bail!("ch_update failed (code {code})"),
        }
    }

    /// Decrypts one CENC buffer with the loaded keys. `subs` holds the clear and cipher byte
    /// count of each subsample one after the other, and is empty when the whole buffer is
    /// encrypted.
    pub fn decrypt(&self, data: &[u8], key_id: &[u8], iv: &[u8], subs: &[u32]) -> Result<Vec<u8>> {
        let mut out = std::ptr::null_mut();
        let mut len = 0u32;
        let code = unsafe {
            ch_decrypt(
                data.as_ptr(),
                data.len() as u32,
                key_id.as_ptr(),
                key_id.len() as u32,
                iv.as_ptr(),
                iv.len() as u32,
                subs.as_ptr(),
                (subs.len() / 2) as u32,
                &mut out,
                &mut len,
            )
        };
        match code {
            0 => Ok(unsafe { take(out, len) }),
            code => bail!("ch_decrypt failed (code {code})"),
        }
    }
}

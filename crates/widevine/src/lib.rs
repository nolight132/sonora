//! Playing Widevine-encrypted fMP4 through the CDM the machine has.
//!
//! Nothing here is about one service. A provider that streams CENC audio needs the same five
//! things, and they are all this crate does:
//!
//! - [`find`] settles which CDM this process uses: one the user named, one in Sonora's own
//!   store, or one a browser on the machine has. [`offer`] and [`Offer::install`] put one into
//!   that store from Google's component update service, the way Chrome and Kodi get theirs,
//!   with Google's terms shown in between. Google publishes nothing anyone may redistribute,
//!   so no release carries the module; every copy is fetched on the machine that uses it.
//! - [`pssh`] builds the init data a CDM wants from a key id.
//! - [`Cdm`] drives the system module: a license challenge, the license back, and samples
//!   decrypted. The device key stays sealed inside it and nothing is read out. The module runs
//!   in a host process of its own, the app's executable started again through [`host`], so its
//!   memory is only spent while a protected track is loaded.
//! - [`cenc`] reads just enough ISO-BMFF to say where every encrypted sample is, what its IV is
//!   and where each fragment starts on the media timeline.
//! - [`cenc::unlock`] relabels the sample entry so an ordinary decoder will open the cleartext.
//!
//! `SONORA_WIDEVINE_CDM` overrides the search with a path of the user's choosing, which is what
//! a package with a CDM of its own should set. With no module anywhere there is no Widevine
//! playback and nothing else changes.
//!
//! The `cdm` feature is what compiles and links the host, the C++ in `shim/`. Without it the
//! parsing, the search and the fetch still compile and [`available`] answers false, so a build
//! with no C++ compiler still has everything but playback.

mod cdm;
pub mod cenc;
mod fetch;
#[cfg(feature = "cdm")]
mod host;
#[cfg(feature = "cdm")]
mod shim;
mod source;
#[cfg(feature = "cdm")]
mod wire;

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

pub use cdm::Cdm;
pub use fetch::{Offer, Release, fetch, latest, offer};
pub use source::{Found, LIBRARY, Origin, configured, find, installed, store, stored, uninstall};

/// The environment variable naming the CDM to load.
pub const CDM_PATH: &str = "SONORA_WIDEVINE_CDM";

/// The environment variable that, when set to anything, skips the browser search. For trying
/// the download on a machine that has a browser's copy.
pub const SKIP_BROWSERS: &str = "SONORA_WIDEVINE_SKIP_BROWSERS";

/// The argument that starts the Sonora executable as the CDM host rather than the app. The
/// module path follows it.
pub const HOST_ARG: &str = "--widevine-host";

/// The Widevine DRM system id, as it appears in a `pssh` box and in an HLS `KEYFORMAT`.
pub const SYSTEM_ID: [u8; 16] = [
    0xed, 0xef, 0x8b, 0xa9, 0x79, 0xd6, 0x4a, 0xce, 0xa3, 0xc8, 0x27, 0xdc, 0xd5, 0x1d, 0x21, 0xed,
];

/// The `KEYFORMAT` a media playlist gives its Widevine key.
pub const KEY_FORMAT: &str = "urn:uuid:edef8ba9-79d6-4ace-a3c8-27dcd51d21ed";

/// Builds the `pssh` box a CDM expects as init data for `key_id`.
///
/// A real CDM parses this as an ISO-BMFF box, so it has to be one: header, the Widevine system
/// id, then a `WidevineCencHeader` protobuf naming the key.
pub fn pssh(key_id: &[u8]) -> Vec<u8> {
    let payload = cenc_header(key_id);
    let total = 4 + 4 + 4 + SYSTEM_ID.len() + 4 + payload.len();
    let mut boxed = Vec::with_capacity(total);
    boxed.extend_from_slice(&(total as u32).to_be_bytes());
    boxed.extend_from_slice(b"pssh");
    boxed.extend_from_slice(&[0, 0, 0, 0]);
    boxed.extend_from_slice(&SYSTEM_ID);
    boxed.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    boxed.extend_from_slice(&payload);
    boxed
}

/// The `WidevineCencHeader` protobuf inside the box, written by hand rather than generated: it
/// is four fields, one of them non-empty, and generating it would put `protoc` on every build
/// machine. Field 1 is the algorithm, 2 the key id, 3 the provider and 6 the policy.
fn cenc_header(key_id: &[u8]) -> Vec<u8> {
    fn varint(out: &mut Vec<u8>, mut value: u64) {
        while value >= 0x80 {
            out.push(value as u8 | 0x80);
            value >>= 7;
        }
        out.push(value as u8);
    }

    let mut out = Vec::with_capacity(key_id.len() + 10);
    out.extend_from_slice(&[0x08, 0x01]);
    out.push(0x12);
    varint(&mut out, key_id.len() as u64);
    out.extend_from_slice(key_id);
    out.extend_from_slice(&[0x1a, 0x00]);
    out.extend_from_slice(&[0x32, 0x00]);
    out
}

/// Held from a challenge to the license that answers it.
static LICENSING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Claims the CDM for one license exchange, waiting for any other to finish first.
///
/// The CDM can only be part way through one license at a time: a second challenge replaces the
/// state the first one's license would be applied to, and that license then comes back
/// `ch_update failed (code 21)`. Two tracks licensing at once is not a rare case, it is what
/// preloading the next track does, so the exchange is serialized here rather than left to each
/// provider to remember.
///
/// Hold it from [`Cdm::challenge`] to [`Cdm::accept`] and then drop it. Decryption needs no
/// such claim: the session keeps the keys of every track licensed through it, which is what
/// lets one track play while the next is licensed.
pub async fn licensing() -> tokio::sync::MutexGuard<'static, ()> {
    LICENSING.lock().await
}

/// The module this process was started to host, when it is the CDM host rather than the app.
/// The executable asks this before anything else and hands the path to [`host`].
pub fn hosted() -> Option<PathBuf> {
    let mut args = std::env::args_os().skip(1);
    match args.next()? == HOST_ARG {
        true => args.next().map(PathBuf::from),
        false => None,
    }
}

/// Runs this process as the CDM host for `module` until the app closes its stdin. The app
/// starts it from [`Cdm::open`], and nothing else should.
#[cfg(feature = "cdm")]
pub fn host(module: &Path) -> Result<()> {
    host::serve(module)
}

/// Fails at once, because this build has no host to run.
#[cfg(not(feature = "cdm"))]
pub fn host(module: &Path) -> Result<()> {
    bail!(
        "this build carries no widevine host, so {} cannot be hosted",
        module.display()
    )
}

/// Whether this build has a host for a CDM. It says nothing about whether the machine has a
/// CDM to open, only whether one could be used if it did.
pub fn supported() -> bool {
    cfg!(feature = "cdm")
}

/// Whether this build and this machine can open a CDM at all. A provider asks before offering
/// playback, so the answer is a missing feature rather than a failed track.
pub fn available() -> bool {
    supported() && find().is_some()
}

/// Fails with the reason a CDM cannot be had, for a caller that wants to say so once rather
/// than per track.
pub fn require() -> Result<()> {
    if !supported() {
        bail!("this build carries no widevine host");
    }
    if find().is_none() {
        bail!("no widevine module was found on this machine");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bytes prost emits for the same message, pinned so the hand-written encoder cannot
    /// drift from what a CDM parses.
    #[test]
    fn the_cenc_header_is_protobuf() {
        assert_eq!(
            cenc_header(&[0xaa; 4]),
            vec![
                0x08, 0x01, 0x12, 0x04, 0xaa, 0xaa, 0xaa, 0xaa, 0x1a, 0x00, 0x32, 0x00
            ]
        );
    }

    #[test]
    fn a_long_key_id_gets_a_multibyte_length() {
        let encoded = cenc_header(&[0u8; 200]);
        assert_eq!(&encoded[2..5], &[0x12, 0xc8, 0x01], "200 as a varint");
        assert_eq!(encoded.len(), 2 + 3 + 200 + 2 + 2);
    }

    #[test]
    fn the_pssh_box_declares_its_own_size() {
        let key_id = [0x11u8; 16];
        let boxed = pssh(&key_id);
        let declared = u32::from_be_bytes(boxed[0..4].try_into().unwrap()) as usize;
        assert_eq!(declared, boxed.len());
        assert_eq!(&boxed[4..8], b"pssh");
        assert_eq!(&boxed[8..12], &[0, 0, 0, 0], "version 0, no flags");
        assert_eq!(&boxed[12..28], &SYSTEM_ID);
        let payload = u32::from_be_bytes(boxed[28..32].try_into().unwrap()) as usize;
        assert_eq!(payload, boxed.len() - 32);
        assert!(boxed[32..].windows(16).any(|window| window == key_id));
    }
}

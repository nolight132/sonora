//! The CDM host process: the Sonora executable started with [`crate::HOST_ARG`]. It loads the
//! module, answers [`crate::wire`] requests from stdin on stdout, and exits when stdin closes.
//!
//! Keeping the module out of the app is what lets its memory go. Widevine maps about 80 MiB the
//! moment it initializes and none of it can be handed back, so the app starts this process
//! while a protected track is loaded and lets it exit once none is.

use std::io::{self, BufWriter, Read, Write};
use std::path::Path;

use anyhow::{Context as _, Result, bail};

use crate::shim::Shim;
use crate::wire;

/// Room for a whole answer, so each one leaves in a single write.
const ANSWER_BUFFER: usize = 64 * 1024;

/// Opens the module at `module` and serves requests until the app closes stdin or goes away.
/// Fails only when the pipe misbehaves some other way or the module does not open, which the
/// app hears about first.
pub fn serve(module: &Path) -> Result<()> {
    name_process();
    let mut input = io::stdin().lock();
    let mut output = BufWriter::with_capacity(ANSWER_BUFFER, answers()?);

    let shim = match Shim::open(module) {
        Ok(shim) => {
            reply(&mut output, wire::OK, wire::VERSION.as_bytes())
                .context("cannot answer the app")?;
            shim
        }
        Err(error) => {
            reply(&mut output, wire::FAILED, format!("{error:#}").as_bytes())
                .context("cannot answer the app")?;
            return Err(error);
        }
    };
    log::info!("widevine: host opened {}", module.display());

    let mut payload = Vec::new();
    let mut subs = Vec::new();
    loop {
        let request = wire::take_header(&mut input).and_then(|(op, len)| {
            payload.resize(len, 0);
            input.read_exact(&mut payload).map(|()| op)
        });
        let op = match request {
            Ok(op) => op,
            Err(error) if gone(&error) => return Ok(()),
            Err(error) => return Err(error).context("cannot read a request"),
        };
        let sent = match answer(&shim, op, &payload, &mut subs) {
            Ok(body) => reply(&mut output, wire::OK, &body),
            Err(error) => reply(&mut output, wire::FAILED, format!("{error:#}").as_bytes()),
        };
        match sent {
            Ok(()) => {}
            Err(error) if gone(&error) => return Ok(()),
            Err(error) => return Err(error).context("cannot answer the app"),
        }
    }
}

/// Runs one request against the module.
fn answer(shim: &Shim, op: u8, payload: &[u8], subs: &mut Vec<u32>) -> Result<Vec<u8>> {
    match op {
        wire::CHALLENGE => shim.challenge(payload),
        wire::UPDATE => shim.update(payload).map(|()| Vec::new()),
        wire::DECRYPT => {
            let Some(request) = wire::split_decrypt(payload, subs) else {
                bail!("the decrypt request is malformed");
            };
            let clear = shim.decrypt(request.sample, request.key_id, request.iv, subs)?;
            if clear.len() != request.sample.len() {
                bail!(
                    "the cdm returned {} bytes for a {} byte sample",
                    clear.len(),
                    request.sample.len()
                );
            }
            Ok(clear)
        }
        op => bail!("no such request ({op})"),
    }
}

/// Sends one answer and flushes it, so the parent is never left waiting on a buffer.
fn reply(output: &mut impl Write, status: u8, body: &[u8]) -> io::Result<()> {
    wire::put_header(output, status, body.len())?;
    output.write_all(body)?;
    output.flush()
}

/// Whether a pipe error means the app closed its end or exited, which ends the host like any
/// other goodbye.
fn gone(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::UnexpectedEof | io::ErrorKind::BrokenPipe
    )
}

/// Where answers go. The module and whatever it links may print, and a stray byte on the
/// answer pipe would put the parent out of step, so on Unix answers leave on a private copy of
/// stdout and fd 1 is pointed at stderr.
#[cfg(unix)]
fn answers() -> Result<std::fs::File> {
    use std::os::fd::FromRawFd as _;

    let private = unsafe { libc::fcntl(libc::STDOUT_FILENO, libc::F_DUPFD_CLOEXEC, 0) };
    if private < 0 {
        return Err(io::Error::last_os_error()).context("cannot copy stdout");
    }
    if unsafe { libc::dup2(libc::STDERR_FILENO, libc::STDOUT_FILENO) } < 0 {
        return Err(io::Error::last_os_error()).context("cannot point stdout at stderr");
    }
    Ok(unsafe { std::fs::File::from_raw_fd(private) })
}

/// Where answers go. The module and whatever it links may print, and a stray byte on the
/// answer pipe would put the parent out of step, so answers leave on a private copy of the
/// stdout handle while the standard handle and the C runtime's descriptor 1 are pointed at
/// stderr, or closed when there is no stderr.
#[cfg(windows)]
fn answers() -> Result<std::fs::File> {
    use std::os::windows::io::FromRawHandle as _;

    use windows_sys::Win32::Foundation::{
        DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let stdout = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    if stdout.is_null() || stdout == INVALID_HANDLE_VALUE {
        bail!("cannot find stdout");
    }
    let mut private: HANDLE = std::ptr::null_mut();
    let process = unsafe { GetCurrentProcess() };
    let copied = unsafe {
        DuplicateHandle(
            process,
            stdout,
            process,
            &mut private,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    };
    if copied == 0 {
        return Err(io::Error::last_os_error()).context("cannot copy stdout");
    }
    unsafe {
        SetStdHandle(STD_OUTPUT_HANDLE, GetStdHandle(STD_ERROR_HANDLE));
        if libc::dup2(2, 1) != 0 {
            libc::close(1);
        }
    }
    Ok(unsafe { std::fs::File::from_raw_handle(private) })
}

/// Where answers go, on a platform with no way here to move the standard handle aside.
#[cfg(not(any(unix, windows)))]
fn answers() -> Result<io::Stdout> {
    Ok(io::stdout())
}

/// Names the process for `ps` and `top`. It is started from `/proc/self/exe`, which would
/// otherwise show up as `exe`.
#[cfg(target_os = "linux")]
fn name_process() {
    unsafe { libc::prctl(libc::PR_SET_NAME, c"sonora-widevine".as_ptr()) };
}

#[cfg(not(target_os = "linux"))]
fn name_process() {}

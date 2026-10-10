use std::time::{Duration, Instant};

use gpui::App;
use state::Sonora;

/// How often freed heap pages go back to the kernel. The first time comes after half of it, while
/// startup is still settling.
const INTERVAL: Duration = Duration::from_secs(10);
/// When clean file pages are shed after launch: once the renderer and drivers are up, again once
/// the first screen has loaded, and once more after the first minute.
const FIRST_SHEDS: [Duration; 3] = [
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(60),
];
/// How often clean file pages are shed after the first time.
const SHED_EVERY: Duration = Duration::from_secs(600);

/// Caps glibc at two malloc arenas and sends every block of 256 KiB or more straight to mmap.
/// Without it each of the many worker threads keeps its own fragmented arena, and freed decode
/// buffers stay resident. Call it first thing in `main`, before any thread exists.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn tune() {
    unsafe {
        libc::mallopt(libc::M_ARENA_MAX, 2);
        libc::mallopt(libc::M_MMAP_THRESHOLD, 256 * 1024);
        libc::mallopt(libc::M_TRIM_THRESHOLD, 1024 * 1024);
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub fn tune() {}

/// Gives freed heap and clean file pages back to the kernel every `INTERVAL`, but only while
/// nobody is watching the window. A trim holds a malloc arena locked while it walks the heap and
/// every page it returns faults back in on the next allocation, so a trim under an animating
/// window drops frames. Work skipped while the window is watched waits for the next quiet tick.
pub fn watch(cx: &mut App) {
    cx.spawn(async move |cx| {
        let started = Instant::now();
        let mut sheds = FIRST_SHEDS.into_iter();
        let mut shed_at = sheds.next().unwrap_or(SHED_EVERY);
        let mut wait = INTERVAL / 2;
        loop {
            cx.background_executor().timer(wait).await;
            wait = INTERVAL;
            if log::log_enabled!(log::Level::Debug) {
                let probed = cx
                    .background_executor()
                    .spawn(async { (footprint().unwrap_or_default(), resident()) })
                    .await;
                cx.update(|cx| report(probed, cx));
            }
            if cx.update(|cx| Sonora::global(cx).wake.read(cx).watched()) {
                continue;
            }
            let shedding = started.elapsed() >= shed_at;
            if shedding {
                shed_at = sheds
                    .next()
                    .unwrap_or_else(|| started.elapsed() + SHED_EVERY);
            }
            cx.background_executor()
                .spawn(async move {
                    release();
                    if shedding {
                        shed();
                    }
                })
                .detach();
        }
    })
    .detach();
}

fn report(probed: (Footprint, Option<usize>), cx: &mut App) {
    let (footprint, resident) = probed;
    let (entries, bytes) = ui::artwork_usage(cx).unwrap_or((0, 0));
    let queue = Sonora::global(cx).queue.read(cx);
    let past = queue.past().len();
    let ahead = queue.upcoming().len() + queue.similar().len();

    log::debug!(
        "memory: rss {}, heap {}, gpu {}, file {}, artwork {entries} entries / {}, queue {past} past / {ahead} ahead",
        resident.map_or_else(|| "unknown".to_owned(), mib),
        mib(footprint.heap),
        mib(footprint.gpu),
        mib(footprint.file),
        mib(bytes)
    );
}

#[derive(Default)]
struct Footprint {
    heap: usize,
    gpu: usize,
    file: usize,
}

#[cfg(target_os = "linux")]
enum Bucket {
    Heap,
    Gpu,
    File,
}

#[cfg(target_os = "linux")]
fn footprint() -> Option<Footprint> {
    Some(tally(&std::fs::read_to_string("/proc/self/smaps").ok()?))
}

#[cfg(target_os = "linux")]
fn tally(smaps: &str) -> Footprint {
    const GPU: [&str; 6] = [
        "/dev/dri",
        "/dev/nvidia",
        "memfd:",
        "dmabuf",
        "/SYSV",
        "amdgpu",
    ];

    let mut footprint = Footprint::default();
    let mut bucket = Bucket::Heap;

    for line in smaps.lines() {
        if let Some(path) = mapping(line) {
            bucket = match path {
                _ if GPU.iter().any(|kind| path.contains(kind)) => Bucket::Gpu,
                "" => Bucket::Heap,
                _ if path.starts_with('[') => Bucket::Heap,
                _ => Bucket::File,
            };
            continue;
        }
        if let Some(rest) = line.strip_prefix("Rss:") {
            let bytes = rest
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0)
                * 1024;

            match bucket {
                Bucket::Heap => footprint.heap += bytes,
                Bucket::Gpu => footprint.gpu += bytes,
                Bucket::File => footprint.file += bytes,
            }
        }
    }

    footprint
}

#[cfg(target_os = "linux")]
fn mapping(line: &str) -> Option<&str> {
    let mut fields = line.split_whitespace();
    let range = fields.next()?;
    let perms = fields.next()?;
    if !range.contains('-') || perms.len() != 4 {
        return None;
    }
    if !perms.ends_with('p') && !perms.ends_with('s') {
        return None;
    }

    Some(fields.nth(3).unwrap_or(""))
}

#[cfg(not(target_os = "linux"))]
fn footprint() -> Option<Footprint> {
    None
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::tally;

    const SMAPS: &str = "\
55d0f4a00000-55d0f4a21000 rw-p 00000000 00:00 0                          [heap]
Rss:                 512 kB
7f9c00000000-7f9c04000000 rw-p 00000000 00:00 0
Rss:                2048 kB
VmFlags: rd wr mr mw me ac
7f9c10000000-7f9c10800000 rw-s 00000000 00:0f 1234       /dev/dri/renderD128
Rss:                4096 kB
7f9c20000000-7f9c20100000 rw-s 00000000 00:01 4321       /memfd:wayland-shm (deleted)
Rss:                1024 kB
7f9c30000000-7f9c30200000 r--p 00000000 103:02 99        /nix/store/libfoo.so
Rss:                 256 kB
7ffd12300000-7ffd12321000 rw-p 00000000 00:00 0          [stack]
Rss:                 128 kB
";

    #[test]
    fn every_mapping_lands_in_a_bucket() {
        let footprint = tally(SMAPS);

        assert_eq!(footprint.heap, (512 + 2048 + 128) * 1024);
        assert_eq!(footprint.gpu, (4096 + 1024) * 1024);
        assert_eq!(footprint.file, 256 * 1024);
    }

    #[test]
    fn counters_ignore_lines_that_only_look_like_mappings() {
        let footprint = tally("VmFlags: rd wr mr mw me ac\nRss:  64 kB\n");

        assert_eq!(footprint.heap, 64 * 1024);
        assert_eq!(footprint.gpu, 0);
        assert_eq!(footprint.file, 0);
    }
}

fn mib(bytes: usize) -> String {
    format!("{:.1} MiB", bytes as f64 / (1024. * 1024.))
}

/// Returns the free pages of every malloc arena to the kernel.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn release() {
    unsafe { libc::malloc_trim(0) };
}

#[cfg(target_os = "macos")]
fn release() {
    unsafe extern "C" {
        fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
    }
    // A null zone with no goal asks every zone to give back all it can.
    unsafe { malloc_zone_pressure_relief(std::ptr::null_mut(), 0) };
}

#[cfg(not(any(all(target_os = "linux", target_env = "gnu"), target_os = "macos")))]
fn release() {}

/// Drops the clean pages of every read-only file mapping from the resident set: code and data of
/// the executable and its libraries that startup touched once. Whatever is still in use faults
/// back in on its next access. A mapping holding any copied-on-write page is skipped.
///
/// The executable and the GPU drivers lose their page table entries outright, which leaves the
/// pages in the page cache even when another process maps them too. A driver is held open while
/// that happens, so it cannot be unloaded and its range handed to memory the call would wipe.
/// Every other library is only paged out, which never discards anything.
#[cfg(target_os = "linux")]
fn shed() {
    let Ok(smaps) = std::fs::read_to_string("/proc/self/smaps") else {
        return;
    };
    let executable = std::env::current_exe().ok();
    let mut current: Option<(usize, usize, &str)> = None;
    for line in smaps.lines() {
        if let Some(path) = mapping(line) {
            current = None;
            let mut fields = line.split_whitespace();
            let (Some(range), Some(perms)) = (fields.next(), fields.next()) else {
                continue;
            };
            if !path.starts_with('/') || path.starts_with("/dev/") || perms.contains('w') {
                continue;
            }
            current = range.split_once('-').and_then(|(start, end)| {
                Some((
                    usize::from_str_radix(start, 16).ok()?,
                    usize::from_str_radix(end, 16).ok()?,
                    path,
                ))
            });
            continue;
        }
        let Some(rest) = line.strip_prefix("Anonymous:") else {
            continue;
        };
        let Some((start, end, path)) = current.take() else {
            continue;
        };
        if rest.split_whitespace().next() != Some("0") {
            continue;
        }
        let (address, length) = (start as *mut libc::c_void, end - start);
        let own = executable
            .as_deref()
            .is_some_and(|executable| executable == std::path::Path::new(path));
        if own {
            unsafe { libc::madvise(address, length, libc::MADV_DONTNEED) };
            continue;
        }
        if driver(path)
            && let Some(_held) = Held::open(path, start)
        {
            unsafe { libc::madvise(address, length, libc::MADV_DONTNEED) };
            continue;
        }
        unsafe { libc::madvise(address, length, libc::MADV_PAGEOUT) };
    }
}

/// A shared library held open with `dlopen`, so it stays mapped until this is dropped.
#[cfg(target_os = "linux")]
struct Held(*mut libc::c_void);

#[cfg(target_os = "linux")]
impl Held {
    /// Holds the library at `path` open if it is still loaded and still mapped at `address`.
    fn open(path: &str, address: usize) -> Option<Self> {
        let name = std::ffi::CString::new(path).ok()?;
        let handle = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOLOAD | libc::RTLD_LAZY) };
        if handle.is_null() {
            return None;
        }
        let held = Self(handle);
        let mut info: libc::Dl_info = unsafe { std::mem::zeroed() };
        let found = unsafe { libc::dladdr(address as *const libc::c_void, &mut info) };
        if found == 0 || info.dli_fname.is_null() {
            return None;
        }
        let loaded = unsafe { std::ffi::CStr::from_ptr(info.dli_fname) };
        let loaded = std::fs::canonicalize(loaded.to_str().ok()?).ok()?;
        (loaded == std::path::Path::new(path)).then_some(held)
    }
}

#[cfg(target_os = "linux")]
impl Drop for Held {
    fn drop(&mut self) {
        unsafe { libc::dlclose(self.0) };
    }
}

/// Whether a library is one of the GPU drivers the renderer opened at startup, or the LLVM they
/// link, most of which a Vulkan renderer never calls.
#[cfg(target_os = "linux")]
fn driver(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    ["libLLVM", "libvulkan_", "libgallium"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

#[cfg(not(target_os = "linux"))]
fn shed() {}

#[cfg(target_os = "linux")]
fn resident() -> Option<usize> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    let kib: usize = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib * 1024)
}

#[cfg(not(target_os = "linux"))]
fn resident() -> Option<usize> {
    None
}

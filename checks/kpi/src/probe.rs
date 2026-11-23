//! What a scenario can observe about itself without strace and without a hook in a store.

use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs,
    mem::MaybeUninit,
    os::fd::OwnedFd,
    path::{Path, PathBuf},
};

use rustix::{fs::inotify, io::Errno};
use stats_alloc::{Region, StatsAlloc, INSTRUMENTED_SYSTEM};

/// Every allocation of this process goes through the counting wrapper of the system allocator.
#[global_allocator]
static ALLOCATOR: &StatsAlloc<std::alloc::System> = &INSTRUMENTED_SYSTEM;

/// Requested allocation bytes, reallocations included, while `operation` ran on any thread.
pub fn allocated<T>(operation: impl FnOnce() -> T) -> (T, u64) {
    let region = Region::new(ALLOCATOR);
    let output = operation();
    let change = region.change();
    let bytes = change.bytes_allocated + change.bytes_reallocated.max(0) as usize;
    (output, bytes as u64)
}

/// `/proc/self/io` of the whole process (every thread).
#[derive(Debug, Clone, Copy, Default)]
pub struct Io {
    /// Bytes passed to read-like syscalls.
    pub rchar: u64,
    /// Bytes passed to write-like syscalls.
    pub wchar: u64,
}

impl Io {
    /// Reads the counters now.
    pub fn now() -> Self {
        let text = fs::read_to_string("/proc/self/io").expect("/proc/self/io is readable");
        let mut io = Self::default();
        for line in text.lines() {
            let mut parts = line.split(':');
            let (Some(name), Some(value)) = (parts.next(), parts.next()) else {
                continue;
            };
            let value: u64 = value.trim().parse().unwrap_or(0);
            match name {
                "rchar" => io.rchar = value,
                "wchar" => io.wchar = value,
                _ => {}
            }
        }
        io
    }

    /// Counters accumulated since `self`, minus the read of `/proc/self/io` itself, which is a
    /// constant per pair of calls and so cancels in a comparison anyway.
    pub fn since(self) -> Self {
        let later = Self::now();
        Self {
            rchar: later.rchar - self.rchar,
            wchar: later.wchar - self.wchar,
        }
    }
}

/// File opens under a set of directories, counted by inotify: `IN_CLOSE_NOWRITE` is a file
/// opened read-only and closed, `IN_CLOSE_WRITE` one opened for writing. Directories are skipped.
pub struct Opens {
    fd: OwnedFd,
    watches: BTreeMap<i32, PathBuf>,
}

/// What [`Opens::finish`] counted.
#[derive(Debug, Default, Clone)]
pub struct OpenCounts {
    /// Read-only opens per file path.
    pub read_only: BTreeMap<PathBuf, u64>,
    /// Opens for writing (any path).
    pub written: u64,
}

impl OpenCounts {
    /// All read-only opens.
    pub fn read_only_total(&self) -> u64 {
        self.read_only.values().sum()
    }

    /// Read-only opens of files whose path contains `fragment` as one component.
    pub fn read_only_under(&self, fragment: &str) -> u64 {
        self.read_only
            .iter()
            .filter(|(path, _)| path.components().any(|part| part.as_os_str() == fragment))
            .map(|(_, count)| count)
            .sum()
    }
}

impl Opens {
    /// Watches `root` and every directory under it that exists now.
    pub fn watch(root: &Path) -> Self {
        let fd = inotify::init(inotify::CreateFlags::NONBLOCK | inotify::CreateFlags::CLOEXEC)
            .expect("inotify is available");
        let mut watches = BTreeMap::new();
        let mut pending = vec![root.to_path_buf()];
        while let Some(directory) = pending.pop() {
            let wd = inotify::add_watch(
                &fd,
                &directory,
                inotify::WatchFlags::CLOSE_NOWRITE | inotify::WatchFlags::CLOSE_WRITE,
            )
            .expect("directory can be watched");
            watches.insert(wd, directory.clone());
            for entry in fs::read_dir(&directory)
                .expect("readable directory")
                .flatten()
            {
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    pending.push(entry.path());
                }
            }
        }
        // Listing the tree above opened each directory; those events carry ISDIR and are skipped.
        Self { fd, watches }
    }

    /// Drains every queued event. The kernel queues an event when the file is closed, so all
    /// closes made before this call are present.
    pub fn finish(self) -> OpenCounts {
        let mut counts = OpenCounts::default();
        let mut buffer = [MaybeUninit::<u8>::uninit(); 64 * 1024];
        let mut reader = inotify::Reader::new(&self.fd, &mut buffer);
        loop {
            match reader.next() {
                Ok(event) => {
                    let flags = event.events();
                    if flags.contains(inotify::ReadFlags::QUEUE_OVERFLOW) {
                        panic!("the inotify queue overflowed; the open count would be short");
                    }
                    if flags.contains(inotify::ReadFlags::ISDIR) {
                        continue;
                    }
                    let Some(name) = event.file_name() else {
                        continue;
                    };
                    let directory = self.watches.get(&event.wd()).cloned().unwrap_or_default();
                    let name = OsStr::new(name.to_str().unwrap_or("?"));
                    if flags.contains(inotify::ReadFlags::CLOSE_NOWRITE) {
                        *counts.read_only.entry(directory.join(name)).or_default() += 1;
                    } else if flags.contains(inotify::ReadFlags::CLOSE_WRITE) {
                        counts.written += 1;
                    }
                }
                Err(Errno::AGAIN) => break,
                Err(Errno::INTR) => continue,
                Err(error) => panic!("reading inotify events failed: {error}"),
            }
        }
        counts
    }
}

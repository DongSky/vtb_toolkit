//! Disk free-space probing — the safety net that keeps unattended
//! recording from filling the disk and corrupting the recording (or the
//! whole machine's ability to function).

use std::path::Path;

/// Default minimum free space before the supervisor stops recording: 2 GiB.
pub const DEFAULT_MIN_FREE: u64 = 2 * 1024 * 1024 * 1024;

/// Free bytes available to unprivileged processes on the filesystem
/// containing `path`.
#[cfg(unix)]
pub fn free_space(path: &Path) -> std::io::Result<u64> {
    use std::os::unix::ffi::OsStrExt;

    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    // f_bavail = blocks available to unprivileged users (respects the
    // reserved-root quota, unlike f_bfree); f_frsize = fragment size.
    Ok((stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64))
}

/// Non-unix: no statvfs available. Report "infinite" free space so the
/// supervisor's low-disk check never fires on platforms where we cannot
/// actually measure it — recording proceeds unguarded rather than being
/// stopped by a probe we can't perform.
#[cfg(not(unix))]
pub fn free_space(_path: &Path) -> std::io::Result<u64> {
    Ok(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_space_reports_nonzero_for_tempdir() {
        let dir = tempfile::tempdir().unwrap();
        let free = free_space(dir.path()).expect("statvfs must succeed");
        assert!(free > 0, "tempdir filesystem should have free space");
    }
}

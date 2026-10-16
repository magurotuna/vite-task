//! A tracked program may pass `-1` as a directory descriptor, typically a
//! failed `open` result that it does not check. With a relative path the call
//! fails with `EBADF`; with an absolute path the descriptor is ignored. The
//! interposers must forward both cases instead of aborting the program.
//!
//! procps-ng's `ps -p <pid>` does this when the process has exited: it opens
//! `/proc/<pid>`, then calls `openat(-1, "status", ...)`.
//!
//! Directory descriptors exist only on Unix; Windows has no equivalent calls.
#![cfg(unix)]

mod test_utils;

use fspy::PathAccess;
use test_log::test;
use test_utils::assert_contains;

fn errno() -> libc::c_int {
    std::io::Error::last_os_error().raw_os_error().unwrap()
}

#[test(tokio::test)]
async fn relative_path_at_invalid_dirfd() -> anyhow::Result<()> {
    track_fn!((), |(): ()| {
        let path = c"hello";
        // SAFETY: every call below receives a valid C string and writable
        // output storage; the kernel rejects the descriptor.
        unsafe {
            assert_eq!(libc::openat(-1, path.as_ptr(), libc::O_RDONLY), -1);
            assert_eq!(errno(), libc::EBADF);

            let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
            assert_eq!(libc::fstatat(-1, path.as_ptr(), stat.as_mut_ptr(), 0), -1);
            assert_eq!(errno(), libc::EBADF);

            assert_eq!(libc::faccessat(-1, path.as_ptr(), libc::F_OK, 0), -1);
            assert_eq!(errno(), libc::EBADF);

            assert!(libc::fdopendir(-1).is_null());
            assert_eq!(errno(), libc::EBADF);
        }
        #[cfg(target_os = "linux")]
        // SAFETY: as above.
        unsafe {
            let mut statx = std::mem::MaybeUninit::<libc::statx>::uninit();
            assert_eq!(
                libc::statx(-1, path.as_ptr(), 0, libc::STATX_BASIC_STATS, statx.as_mut_ptr()),
                -1
            );
            assert_eq!(errno(), libc::EBADF);

            assert_eq!(
                libc::syscall(
                    libc::SYS_statx,
                    -1,
                    path.as_ptr(),
                    0,
                    libc::STATX_BASIC_STATS,
                    statx.as_mut_ptr(),
                ),
                -1
            );
            assert_eq!(errno(), libc::EBADF);
        }
    })
    .await?;
    Ok(())
}

#[test(tokio::test)]
async fn absolute_path_at_invalid_dirfd() -> anyhow::Result<()> {
    let tmp_dir = tempfile::tempdir()?;
    let tmp_path = std::fs::canonicalize(tmp_dir.path())?.join("hello");
    std::fs::write(&tmp_path, "")?;

    let accesses = track_fn!(tmp_path.to_str().unwrap().to_owned(), |tmp_path: String| {
        let path = std::ffi::CString::new(tmp_path).unwrap();
        // SAFETY: `path` is a valid C string; an absolute path makes the
        // kernel ignore the descriptor.
        let fd = unsafe { libc::openat(-1, path.as_ptr(), libc::O_RDONLY) };
        assert!(fd >= 0, "openat failed with errno {}", errno());
        // SAFETY: `fd` was opened above and is closed once.
        unsafe { libc::close(fd) };
    })
    .await?;
    assert_contains(&accesses, &tmp_path, PathAccess::Read);

    Ok(())
}

use chainerror::*;
use itertools::{EitherOrBoth, Itertools};
use libc::ioctl;
use std::fs::File;
use std::{
    ffi::OsString,
    io,
    os::unix::fs::symlink,
    os::unix::io::AsRawFd,
    path::{Path, PathBuf},
};
use crate::util::acl::acl_copy_fd;

pub fn canonicalize_dir(source: &Path) -> ChainResult<PathBuf, String> {
    let source_filename = source
        .file_name()
        .ok_or_else(|| strerr!("cant get filename"))?;

    let mut source = source
        .parent()
        .ok_or_else(|| strerr!("cant get parent"))?
        .canonicalize()
        .map_err(mstrerr!("Can't canonicalize"))?;

    source.push(source_filename);
    Ok(source)
}

pub fn convert_abs_rel(source: &Path, target: &Path) -> ChainResult<PathBuf, String> {
    let mut target_rel = PathBuf::new();
    let mut rest = PathBuf::new();

    source
        .components()
        .zip_longest(
            target
                .parent()
                .ok_or_else(|| strerr!("cant get parent"))?
                .components(),
        )
        .filter_map(|v| match v {
            EitherOrBoth::Both(a, b) => {
                if a == b {
                    Some((OsString::new(), OsString::new()))
                } else {
                    Some((OsString::from(a.as_os_str()), OsString::from("..")))
                }
            }
            EitherOrBoth::Left(a) => Some((OsString::from(a.as_os_str()), OsString::new())),
            EitherOrBoth::Right(_) => None,
        })
        .for_each(|(a, b)| {
            rest.push(a);
            target_rel.push(b);
        });

    target_rel.push(rest);

    Ok(target_rel)
}

pub fn ln_r(source: &Path, target: &Path) -> ChainResult<(), String> {
    let target = convert_abs_rel(source, target)?;
    symlink(source, target).map_err(mstrerr!("Can't symlink"))?;
    Ok(())
}

pub fn clone_file(source: &Path, target: &Path) -> ChainResult<(), String> {
    unsafe fn fi_clone(fd: libc::c_int, data: libc::c_ulong) -> io::Result<()> {
        if ioctl(fd, 0x40_04_94_09, data) != 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    let source = File::open(source).map_err(mstrerr!("Failed to open source"))?;
    let target = File::create(target).map_err(mstrerr!("Failed to open target"))?;
    unsafe { fi_clone(target.as_raw_fd(), source.as_raw_fd() as u64) }
        .map_err(mstrerr!("Can't clone"))
}

pub fn cp(from: &Path, to: &Path) -> io::Result<u64> {
    use std::cmp;
    use std::fs::File;
    use std::ptr;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn cvt(t: i64) -> io::Result<i64> {
        if t == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(t)
        }
    }

    // Kernel prior to 4.5 don't have copy_file_range
    // We store the availability in a global to avoid unnecessary syscalls
    static HAS_COPY_FILE_RANGE: AtomicBool = AtomicBool::new(true);

    unsafe fn copy_file_range(
        fd_in: libc::c_int,
        off_in: *mut libc::loff_t,
        fd_out: libc::c_int,
        off_out: *mut libc::loff_t,
        len: libc::size_t,
        flags: libc::c_uint,
    ) -> libc::c_long {
        libc::syscall(
            libc::SYS_copy_file_range,
            fd_in,
            off_in,
            fd_out,
            off_out,
            len,
            flags,
        )
    }

    if !from.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the source path is not an existing regular file",
        ));
    }

    let umask = unsafe { libc::umask(0) };

    let mut reader = File::open(from)?;
    let mut writer = File::create(to)?;
    let (perm, len) = {
        let metadata = reader.metadata()?;
        (metadata.permissions(), metadata.len())
    };

    let has_copy_file_range = HAS_COPY_FILE_RANGE.load(Ordering::Relaxed);
    let mut written = 0u64;
    while written < len {
        let copy_result = if has_copy_file_range {
            let bytes_to_copy = cmp::min(len - written, usize::max_value() as u64) as usize;
            let copy_result = unsafe {
                // We actually don't have to adjust the offsets,
                // because copy_file_range adjusts the file offset automatically
                cvt(copy_file_range(
                    reader.as_raw_fd(),
                    ptr::null_mut(),
                    writer.as_raw_fd(),
                    ptr::null_mut(),
                    bytes_to_copy,
                    0,
                ))
            };
            if let Err(ref copy_err) = copy_result {
                match copy_err.raw_os_error() {
                    Some(libc::ENOSYS) | Some(libc::EPERM) => {
                        HAS_COPY_FILE_RANGE.store(false, Ordering::Relaxed);
                    }
                    _ => {}
                }
            }
            copy_result
        } else {
            Err(io::Error::from_raw_os_error(libc::ENOSYS))
        };
        match copy_result {
            Ok(ret) => written += ret as u64,
            Err(err) => {
                match err.raw_os_error() {
                    Some(os_err)
                        if os_err == libc::ENOSYS
                            || os_err == libc::EXDEV
                            || os_err == libc::EPERM =>
                    {
                        // Try fallback io::copy if either:
                        // - Kernel version is < 4.5 (ENOSYS)
                        // - Files are mounted on different fs (EXDEV)
                        // - copy_file_range is disallowed, for example by seccomp (EPERM)
                        assert_eq!(written, 0);
                        let ret = io::copy(&mut reader, &mut writer)?;
                        writer.set_permissions(perm)?;
                        return Ok(ret);
                    }
                    _ => return Err(err),
                }
            }
        }
    }
    writer.set_permissions(perm)?;

    use super::acl_copy_fd;

    unsafe { libc::umask(umask) };

    acl_copy_fd(reader.as_raw_fd(), writer.as_raw_fd())?;

    Ok(written)
}

#[cfg(test)]
mod test {
    use std::ffi::OsString;
    use std::fs::File;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_convert_abs_rel() {
        use super::{canonicalize_dir, convert_abs_rel};
        assert_eq!(
            convert_abs_rel(
                &canonicalize_dir(&PathBuf::from("/lib64/libc.so.6")).unwrap(),
                &canonicalize_dir(&PathBuf::from("/usr/libexec/libc.so")).unwrap(),
            )
            .unwrap(),
            OsString::from("../lib64/libc.so.6")
        );
        assert_eq!(
            convert_abs_rel(
                &PathBuf::from("/lib64/libc.so.6"),
                &PathBuf::from("/var/libc.so"),
            )
            .unwrap(),
            OsString::from("../lib64/libc.so.6")
        );
        assert_eq!(
            convert_abs_rel(
                &PathBuf::from("/lib64/libc.so.6"),
                &PathBuf::from("/libc.so"),
            )
            .unwrap(),
            OsString::from("lib64/libc.so.6")
        );
    }

    #[test]
    fn test_clone() {
        use super::clone_file;
        use std::error::Error;
        use std::io::Read;
        use std::io::Write;

        let txt = "Test contents";
        let tmp_dir = TempDir::new().unwrap();
        let file_path = tmp_dir.path().join("my-temporary-note.txt");
        let file_path2 = tmp_dir.path().join("my-temporary-note2.txt");
        {
            let mut tmp_file = File::create(&file_path).unwrap();
            tmp_file.write_all(txt.as_bytes()).unwrap();
        }

        let res = clone_file(&file_path, &file_path2).and_then(|_| {
            let mut tmp_file2 = File::open(file_path2).unwrap();
            let mut s = Vec::new();
            tmp_file2.read_to_end(&mut s).unwrap();
            assert_eq!(String::from_utf8_lossy(&s), txt);
            Ok(())
        });
        if let Err(e) = res {
            eprintln!("{:#?}", e.source());
        }
    }

    #[test]
    fn test_cp() {
        use super::cp;
        use std::io::Read;
        use std::io::Write;

        let txt = "Test contents";
        let tmp_dir = TempDir::new().unwrap();
        let file_path = tmp_dir.path().join("my-temporary-note.txt");
        let file_path2 = tmp_dir.path().join("my-temporary-note2.txt");
        {
            let mut tmp_file = File::create(&file_path).unwrap();
            tmp_file.write_all(txt.as_bytes()).unwrap();
        }

        cp(&file_path, &file_path2).unwrap();

        let mut tmp_file2 = File::open(file_path2).unwrap();
        let mut s = Vec::new();
        tmp_file2.read_to_end(&mut s).unwrap();
        assert_eq!(String::from_utf8_lossy(&s), txt);
    }
}

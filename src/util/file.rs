use chainerror::*;
use itertools::{EitherOrBoth, Itertools};
use libc::ioctl;
use libc::{fstat64, stat64};
use std::fs::File;
use std::{
    cmp,
    ffi::OsString,
    io,
    io::Read,
    io::Write,
    mem,
    os::unix::fs::symlink,
    os::unix::io::AsRawFd,
    os::unix::io::RawFd,
    path::{Path, PathBuf},
};

use crate::util::acl::acl_copy_fd;

#[doc(hidden)]
pub trait IsMinusOne {
    fn is_minus_one(&self) -> bool;
}

macro_rules! impl_is_minus_one {
    ($($t:ident)*) => ($(impl IsMinusOne for $t {
        fn is_minus_one(&self) -> bool {
            *self == -1
        }
    })*)
}

impl_is_minus_one! { i8 i16 i32 i64 isize }

pub fn cvt<T: IsMinusOne>(t: T) -> io::Result<T> {
    if t.is_minus_one() {
        Err(io::Error::last_os_error())
    } else {
        Ok(t)
    }
}

pub fn cvt_ignore_perm<T: IsMinusOne + From<i8>>(t: T, ignore_eperm: bool) -> io::Result<T> {
    if t.is_minus_one() {
        let e = io::Error::last_os_error();
        if ignore_eperm {
            if let Some(libc::EPERM) = e.raw_os_error() {
                Ok(T::from(0i8))
            } else {
                Err(e)
            }
        } else {
            Err(e)
        }
    } else {
        Ok(t)
    }
}

pub fn file_attr(fd: RawFd) -> io::Result<stat64> {
    let mut stat: stat64 = unsafe { std::mem::zeroed() };
    cvt(unsafe { fstat64(fd, &mut stat) })?;
    Ok(stat)
}

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

pub fn cp(from: &Path, to: &Path) -> ChainResult<u64, io::Error> {
    use std::fs::File;
    use std::sync::atomic::{AtomicBool, Ordering};

    // Kernel prior to 4.5 don't have copy_file_range
    // We store the availability in a global to avoid unnecessary syscalls
    static HAS_COPY_FILE_RANGE: AtomicBool = AtomicBool::new(true);
    static HAS_SENDFILE: AtomicBool = AtomicBool::new(true);

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

    fn cp_with_holes(reader: &mut File, writer: &mut File, bytes_to_copy: i64) -> io::Result<u64> {
        let mut has_copy_file_range = HAS_COPY_FILE_RANGE.load(Ordering::Relaxed);
        let mut has_sendfile = HAS_SENDFILE.load(Ordering::Relaxed);
        let mut can_handle_holes = true;

        let fd_in = reader.as_raw_fd();
        let fd_out = writer.as_raw_fd();

        cvt(unsafe { libc::ftruncate(fd_out, bytes_to_copy as i64) }).unwrap_or_else(|_| {
            can_handle_holes = false;
            0
        });

        let mut srcpos: i64 = 0;

        let mut next_beg: libc::loff_t = if can_handle_holes {
            cvt(unsafe { libc::lseek(fd_in, srcpos, libc::SEEK_DATA) }).unwrap_or_else(|_| {
                can_handle_holes = false;
                0
            })
        } else {
            0
        };

        let mut next_end: libc::loff_t = if can_handle_holes {
            cvt(unsafe { libc::lseek(fd_in, next_beg, libc::SEEK_HOLE) }).unwrap_or_else(|_| {
                can_handle_holes = false;
                bytes_to_copy
            })
        } else {
            bytes_to_copy
        };

        let mut next_len = next_end - next_beg;

        while srcpos < bytes_to_copy {
            if srcpos != 0 {
                if can_handle_holes {
                    next_beg = cvt(unsafe { libc::lseek(fd_in, srcpos, libc::SEEK_DATA) })?;
                    next_end = cvt(unsafe { libc::lseek(fd_in, next_beg, libc::SEEK_HOLE) })?;

                    next_len = next_end - next_beg;
                } else {
                    next_beg = srcpos;
                    next_end = bytes_to_copy - srcpos;
                }
            }

            if next_len <= 0 {
                srcpos = next_end;
                continue;
            }

            let num = if has_copy_file_range {
                match cvt(unsafe {
                    copy_file_range(
                        fd_in,
                        &mut next_beg,
                        fd_out,
                        &mut next_beg,
                        next_len as usize,
                        0,
                    )
                }) {
                    Ok(n) => n as isize,
                    Err(copy_err) => match copy_err.raw_os_error() {
                        Some(libc::ENOSYS) | Some(libc::EPERM) => {
                            HAS_COPY_FILE_RANGE.store(false, Ordering::Relaxed);
                            has_copy_file_range = false;
                            continue;
                        }
                        Some(libc::EXDEV) => {
                            has_copy_file_range = false;
                            continue;
                        }
                        _ => {
                            return Err(copy_err);
                        }
                    },
                }
            } else if has_sendfile {
                if can_handle_holes {
                    if next_beg != 0 {
                        cvt(unsafe { libc::lseek(fd_out, next_beg, libc::SEEK_SET) })?;
                    }
                }
                match cvt(unsafe {
                    libc::sendfile(fd_out, fd_in, &mut next_beg, next_len as usize)
                }) {
                    Ok(n) => n,
                    Err(copy_err) => match copy_err.raw_os_error() {
                        Some(libc::ENOSYS) => {
                            HAS_SENDFILE.store(false, Ordering::Relaxed);
                            has_sendfile = false;
                            continue;
                        }
                        Some(libc::EINVAL) => {
                            has_sendfile = false;
                            continue;
                        }
                        _ => {
                            return Err(copy_err);
                        }
                    },
                }
            } else {
                if can_handle_holes {
                    cvt(unsafe { libc::lseek(fd_in, next_beg, libc::SEEK_SET) })?;
                    if next_beg != 0 {
                        cvt(unsafe { libc::lseek(fd_out, next_beg, libc::SEEK_SET) })?;
                    }
                }
                // const DEFAULT_BUF_SIZE: usize = ::sys_common::io::DEFAULT_BUF_SIZE;
                const DEFAULT_BUF_SIZE: usize = 8 * 1024;
                let mut buf = unsafe {
                    let buf: [u8; DEFAULT_BUF_SIZE] = mem::uninitialized();
                    buf
                };

                let mut written = 0;
                while next_len > 0 {
                    let slice_len = cmp::min(next_len as usize, DEFAULT_BUF_SIZE);
                    let len = match reader.read(&mut buf[..slice_len]) {
                        Ok(0) => return Ok(srcpos as u64 + written),
                        Ok(len) => len,
                        Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                        Err(e) => return Err(e),
                    };
                    writer.write_all(&buf[..len])?;
                    written += len as u64;
                    next_len -= len as i64;
                }
                written as isize
            };
            srcpos += num as i64;
        }
        Ok(srcpos as u64)
    }

    if !from.is_file() {
        return Err(cherr!(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the source path is not an existing regular file",
        )));
    }

    let umask = unsafe { libc::umask(0) };

    let mut reader = File::open(from).map_err(minto_cherr!())?;
    let mut writer = File::create(to).map_err(minto_cherr!())?;
    let stat = file_attr(reader.as_raw_fd()).map_err(minto_cherr!())?;

    let len = stat.st_size;

    let written = cp_with_holes(&mut reader, &mut writer, len).map_err(minto_cherr!())?;

    let ignore_eperm = unsafe { libc::geteuid() != 0 };

    cvt(unsafe { libc::fchmod(writer.as_raw_fd(), stat.st_mode) }).map_err(minto_cherr!())?;

    cvt_ignore_perm(
        unsafe { libc::fchown(writer.as_raw_fd(), stat.st_uid, stat.st_gid) },
        ignore_eperm,
    )
    .map_err(minto_cherr!())?;

    acl_copy_fd(reader.as_raw_fd(), writer.as_raw_fd(), ignore_eperm)?;

    unsafe { libc::umask(umask) };

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
        use std::io::{self, Read, Write};

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
            if let Some(io_err) = e.find_kind_or_cause::<io::Error>() {
                match io_err.raw_os_error() {
                    Some(libc::EOPNOTSUPP) => {}
                    _ => panic!("\n{}\n", e),
                }
            }
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

        match cp(&file_path, &file_path2) {
            Ok(size) if size as usize != txt.len() => {
                panic!("copied only {} bytes", size);
            }

            Err(e) => panic!("{:?}", e),
            _ => {}
        }
        let mut tmp_file2 = File::open(file_path2).unwrap();
        let mut s = Vec::new();
        tmp_file2.read_to_end(&mut s).unwrap();
        assert_eq!(String::from_utf8_lossy(&s), txt);
    }

    #[test]
    fn test_cp_acl() {
        use super::cp;

        cp(
            &PathBuf::from("/usr/bin/ping"),
            &PathBuf::from("/var/tmp/ping"),
        )
        .map_err(|e| panic!("\n{:?}\n", e))
        .unwrap();

        cp(
            &PathBuf::from("/var/tmp/ping"),
            &PathBuf::from("/var/tmp/ping2"),
        )
        .map_err(|e| panic!("\n{:?}\n", e))
        .unwrap();

        cp(&PathBuf::from("/var/tt/ping"), &PathBuf::from("/efi/ping"))
            .map_err(|e| panic!("\n{:?}\n", e))
            .unwrap();
        cp(&PathBuf::from("/efi/ping"), &PathBuf::from("/efi/ping2"))
            .map_err(|e| panic!("\n{:?}\n", e))
            .unwrap();
    }
}

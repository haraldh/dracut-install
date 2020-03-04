use std::cmp;
use std::ffi::{OsStr, OsString};
use std::io::{self, Error, ErrorKind};
use std::os::unix::fs::symlink;
use std::os::unix::prelude::*;
use std::path::{Path, PathBuf};
use std::{fs, mem, os, sync};

use chainerror::*;
use itertools::{EitherOrBoth, Itertools};
use libc::{fstat64, ftruncate64, lseek64, stat64};

use crate::acl::acl_copy_fd;

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

pub fn cvt_r<T, F>(mut f: F) -> io::Result<T>
where
    T: IsMinusOne,
    F: FnMut() -> T,
{
    loop {
        match cvt(f()) {
            Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
            other => return other,
        }
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
    let source = convert_abs_rel(source, target)?;
    symlink(&source, &target).map_err(mstrerr!("Can't symlink {:?} to {:?}", source, target))?;
    Ok(())
}

pub fn clone_path(
    source: &Path,
    root_dir: &Path,
) -> Result<(), Box<dyn std::error::Error + 'static + Send + Sync>> {
    use os::unix::fs::DirBuilderExt;
    use std::fs::DirBuilder;

    let mut target = PathBuf::from(root_dir);

    if source.has_root() {
        let source = &source.as_os_str().as_bytes()[1..];
        target.push(OsStr::from_bytes(source));
    } else {
        target.push(source);
    }

    eprintln!("clone_path {:?} {:?}", source, target);

    if target.symlink_metadata().is_ok() {
        return Ok(());
    }

    match source.parent() {
        Some(s) => clone_path(s, root_dir)?,
        _ => return Ok(()),
    }

    let source_metadata = source
        .symlink_metadata()
        .map_err(mstrerr!("Failed to get symlink metadata"))?;
    let source_perms = source_metadata.permissions();
    let target_parent = target.parent();
    let target_parent_perms = target_parent
        .and_then(|p| p.metadata().ok())
        .map(|p| p.permissions())
        .filter(|p| p.readonly());

    if let (Some(target_parent), Some(ref tp)) = (target_parent, &target_parent_perms) {
        let mut tp = tp.clone();
        tp.set_readonly(false);
        std::fs::set_permissions(target_parent, tp)
            .map_err(mstrerr!("Failed to set permissions"))?;
    }

    let ret = if source_metadata.file_type().is_symlink() {
        let mut path =
            fs::read_link(source).map_err(mstrerr!("Failed to read link of {:#?}", source))?;
        if !path.has_root() {
            let mut sp = PathBuf::from(
                source
                    .parent()
                    .unwrap_or_else(|| std::path::Component::RootDir.as_ref()),
            );
            sp.push(path);
            path = sp;
        }
        clone_path(&path, root_dir)?;
        eprintln!("clone_path symlink {:?} {:?}", path, target);

        let mut target_path = PathBuf::from(root_dir);
        if path.has_root() {
            let path = &path.as_os_str().as_bytes()[1..];
            target_path.push(OsStr::from_bytes(path));
        } else {
            target_path.push(path);
        }
        eprintln!("ln_r symlink {:?} {:?}", target_path, target);

        ln_r(&target_path, &target).map_err(mstrerr!(
            "failed ln_r symlink {:?} {:?}",
            target_path,
            target
        ))
    } else if source.is_dir() {
        eprintln!("clone_path mkdir {:?} {:?}", source, target);
        let mut builder = DirBuilder::new();
        builder.mode(source_perms.mode());
        builder
            .create(&target)
            .map_err(mstrerr!("clone_path mkdir {:?} {:?}", source, target))
    } else if source.is_file() {
        eprintln!("clone_path copy {:?} {:?}", source, target);
        copy(source, &target).map(|_| ()).map_err(mstrerr!(
            "clone_path copy {:?} {:?}",
            source,
            target
        ))
    } else {
        unimplemented!()
    };

    if let (Some(target_parent), Some(ref tp)) = (target_parent, &target_parent_perms) {
        std::fs::set_permissions(target_parent, tp.clone()).map_err(mstrerr!("set_permissions"))?;
    }

    ret?;
    Ok(())
}

pub fn copy(from: &Path, to: &Path) -> ChainResult<u64, io::Error> {
    use fs::{File, OpenOptions};
    use io::{Read, Write};
    use sync::atomic::{AtomicBool, Ordering};

    // Kernel prior to 4.5 don't have copy_file_range
    // We store the availability in a global to avoid unnecessary syscalls
    static HAS_COPY_FILE_RANGE: AtomicBool = AtomicBool::new(true);
    // Kernel prior to 2.2 don't have sendfile
    // We store the availability in a global to avoid unnecessary syscalls
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

    let mut reader = File::open(from).map_err(|e| cherr!(e))?;

    let (perms, len) = {
        let metadata = reader.metadata().map_err(|e| cherr!(e))?;
        if !metadata.is_file() {
            return Err(cherr!(Error::new(
                ErrorKind::InvalidInput,
                "the source path is not an existing regular file",
            )));
        }
        (metadata.permissions(), metadata.len())
    };
    let bytes_to_copy: i64 = len as i64;

    let mut writer = OpenOptions::new()
        // create the new file with the correct mode
        .mode(perms.mode())
        .write(true)
        .create(true)
        .truncate(true)
        .open(to)
        .map_err(|e| cherr!(e))?;

    let mut can_handle_sparse = true;

    let fd_in = reader.as_raw_fd();
    let fd_out = writer.as_raw_fd();

    let writer_metadata = writer.metadata().map_err(|e| cherr!(e))?;
    // prevent root from setting permissions on e.g. `/dev/null`
    // prevent users from setting permissions on e.g. `/dev/stdout` or a named pipe
    if writer_metadata.is_file() {
        writer.set_permissions(perms).map_err(|e| cherr!(e))?;

        let ignore_eperm = unsafe { libc::geteuid() != 0 };

        let stat = file_attr(reader.as_raw_fd()).map_err(|e| cherr!(e))?;

        cvt_ignore_perm(
            unsafe { libc::fchown(writer.as_raw_fd(), stat.st_uid, stat.st_gid) },
            ignore_eperm,
        )
        .map_err(|e| cherr!(e))?;

        acl_copy_fd(reader.as_raw_fd(), writer.as_raw_fd(), ignore_eperm)?;

        match cvt_r(|| unsafe { ftruncate64(fd_out, bytes_to_copy) }) {
            Ok(_) => {}
            Err(err) => match err.raw_os_error() {
                Some(libc::EINVAL) => {
                    can_handle_sparse = false;
                }
                _ => {
                    return Err(cherr!(err));
                }
            },
        }
    } else {
        can_handle_sparse = false;
    }

    let mut use_copy_file_range = HAS_COPY_FILE_RANGE.load(Ordering::Relaxed);
    let mut use_sendfile = HAS_SENDFILE.load(Ordering::Relaxed);

    let mut srcpos: i64 = 0;

    let mut next_beg: libc::loff_t = if can_handle_sparse {
        let ret = unsafe { lseek64(fd_in, srcpos, libc::SEEK_DATA) };
        if ret == -1 {
            can_handle_sparse = false;
            0
        } else {
            ret
        }
    } else {
        0
    };

    let mut next_end: libc::loff_t = if can_handle_sparse {
        let ret = unsafe { lseek64(fd_in, next_beg, libc::SEEK_HOLE) };
        if ret == -1 {
            can_handle_sparse = false;
            bytes_to_copy
        } else {
            ret
        }
    } else {
        bytes_to_copy
    };

    let mut next_len = next_end - next_beg;

    while srcpos < bytes_to_copy {
        if srcpos != 0 {
            if can_handle_sparse {
                next_beg = cvt(unsafe { lseek64(fd_in, srcpos, libc::SEEK_DATA) })
                    .map_err(|e| cherr!(e))?;
                next_end = cvt(unsafe { lseek64(fd_in, next_beg, libc::SEEK_HOLE) })
                    .map_err(|e| cherr!(e))?;

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

        let num = if use_copy_file_range {
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
                Err(err) => match err.raw_os_error() {
                    // Try fallback if either:
                    // - Kernel version is < 4.5 (ENOSYS)
                    // - Files are mounted on different fs (EXDEV)
                    // - copy_file_range is disallowed, for example by seccomp (EPERM)
                    Some(libc::ENOSYS) | Some(libc::EPERM) => {
                        HAS_COPY_FILE_RANGE.store(false, Ordering::Relaxed);
                        use_copy_file_range = false;
                        continue;
                    }
                    Some(libc::EXDEV) | Some(libc::EINVAL) => {
                        use_copy_file_range = false;
                        continue;
                    }
                    _ => {
                        return Err(cherr!(err));
                    }
                },
            }
        } else if use_sendfile {
            if can_handle_sparse && next_beg != 0 {
                cvt(unsafe { lseek64(fd_out, next_beg, libc::SEEK_SET) }).map_err(|e| cherr!(e))?;
            }
            match cvt(unsafe { libc::sendfile(fd_out, fd_in, &mut next_beg, next_len as usize) }) {
                Ok(n) => n,
                Err(err) => match err.raw_os_error() {
                    // Try fallback if either:
                    // - Kernel version is < 2.2 (ENOSYS)
                    // - sendfile is disallowed, for example by seccomp (EPERM)
                    // - can't use sendfile on source or destination (EINVAL)
                    Some(libc::ENOSYS) | Some(libc::EPERM) => {
                        HAS_SENDFILE.store(false, Ordering::Relaxed);
                        use_sendfile = false;
                        continue;
                    }
                    Some(libc::EINVAL) => {
                        use_sendfile = false;
                        continue;
                    }
                    _ => {
                        return Err(cherr!(err));
                    }
                },
            }
        } else {
            if can_handle_sparse {
                cvt(unsafe { lseek64(fd_in, next_beg, libc::SEEK_SET) }).map_err(|e| cherr!(e))?;
                if next_beg != 0 {
                    cvt(unsafe { lseek64(fd_out, next_beg, libc::SEEK_SET) })
                        .map_err(|e| cherr!(e))?;
                }
            }
            //const DEFAULT_BUF_SIZE: usize = ::sys_common::io::DEFAULT_BUF_SIZE;
            const DEFAULT_BUF_SIZE: usize = 8 * 1024;
            let mut buf = unsafe {
                let buf: [u8; DEFAULT_BUF_SIZE] = mem::MaybeUninit::uninit().assume_init();
                buf
            };

            let mut written = 0;
            while next_len > 0 {
                let slice_len = cmp::min(next_len as usize, DEFAULT_BUF_SIZE);
                let len = match reader.read(&mut buf[..slice_len]) {
                    Ok(0) => {
                        // break early out of copy loop, because nothing is to be read anymore
                        srcpos += written;
                        break;
                    }
                    Ok(len) => len,
                    Err(ref err) if err.kind() == io::ErrorKind::Interrupted => continue,
                    Err(err) => return Err(cherr!(err)),
                };
                writer.write_all(&buf[..len]).map_err(|e| cherr!(e))?;
                written += len as i64;
                next_len -= len as i64;
            }
            written as isize
        };
        srcpos += num as i64;
    }

    Ok(srcpos as u64)
}

#[cfg(test)]
mod test {
    use std::ffi::OsString;
    use std::fs::File;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

    use tempfile::TempDir;

    #[test]
    fn test_convert_abs_rel() {
        use super::{canonicalize_dir, convert_abs_rel};
        let tmp_dir = TempDir::new().unwrap();
        let lib64 = tmp_dir.path().join("usr").join("lib64");
        let libexec = tmp_dir.path().join("usr").join("libexec");

        ::std::fs::create_dir_all(&lib64).unwrap();

        let libc_so_6 = lib64.join("libc.so.6");
        File::create(&libc_so_6).unwrap();

        symlink(&lib64, &tmp_dir.path().join("lib64")).unwrap();

        assert_eq!(
            convert_abs_rel(
                &canonicalize_dir(&libc_so_6).unwrap(),
                &libexec.join("libc.so"),
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
    fn test_cp() {
        use super::copy;
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

        match copy(&file_path, &file_path2) {
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
    fn test_copy_null() {
        use super::copy;

        copy(&PathBuf::from("/usr/bin/ping"), &PathBuf::from("/dev/null"))
            .map_err(|e| panic!("\n{:?}\n", e))
            .unwrap();
    }

    #[test]
    fn test_copy_tmp_small() {
        use super::copy;
        let tmp_dir = TempDir::new_in("/tmp").unwrap();
        let dst = tmp_dir.path().join("ping");
        copy(&PathBuf::from("/usr/bin/ping"), &dst)
            .map_err(|e| panic!("\n{:?}\n", e))
            .unwrap();
    }

    #[test]
    fn test_cp_acl() {
        use super::copy;

        let tmp_dir = TempDir::new_in("/var/tmp").unwrap();
        let dst = tmp_dir.path().join("ping");
        let dst2 = tmp_dir.path().join("ping2");

        copy(&PathBuf::from("/usr/bin/ping"), &dst)
            .map_err(|e| panic!("\n{:?}\n", e))
            .unwrap();

        copy(&dst, &dst2)
            .map_err(|e| panic!("\n{:?}\n", e))
            .unwrap();
    }
}

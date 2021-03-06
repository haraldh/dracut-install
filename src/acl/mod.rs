use chainerror::prelude::v1::*;
use libc::{fgetxattr, flistxattr, fsetxattr};
use std::io;
use std::os::unix::io::RawFd;
use std::ptr;

use crate::cstrviter::CStrVIterator;

pub fn acl_copy_fd(fd_in: RawFd, fd_out: RawFd, ignore_eperm: bool) -> ChainResult<(), String> {
    let num_bytes = unsafe {
        match flistxattr(fd_in, ptr::null_mut(), 0) {
            t if t < 0 => {
                let err = io::Error::last_os_error();
                return match err.raw_os_error() {
                    Some(libc::ENODATA) | Some(libc::EOPNOTSUPP) => Ok(()),
                    _ => Err(err).context("acl_copy_fd num_bytes".into()),
                };
            }
            t if t > 0 => t,
            _ => return Ok(()),
        }
    };
    let mut names = Vec::<u8>::with_capacity(num_bytes as usize);
    unsafe {
        match flistxattr(fd_in, names.as_mut_ptr() as *mut i8, num_bytes as usize) {
            t if t < 0 => {
                let err = io::Error::last_os_error();
                return match err.raw_os_error() {
                    Some(libc::ENODATA) | Some(libc::EOPNOTSUPP) => Ok(()),
                    _ => Err(err).context("acl_copy_fd names".into()),
                };
            }
            t => names.set_len(t as usize),
        };
    };

    for name in CStrVIterator::from_bytes(&names) {
        let t_str_bytes_ptr = name.as_ptr();
        unsafe {
            let num_bytes = fgetxattr(fd_in, t_str_bytes_ptr as *const i8, ptr::null_mut(), 0);
            if num_bytes < 0 {
                let err = io::Error::last_os_error();
                return match err.raw_os_error() {
                    Some(libc::ENODATA) | Some(libc::EOPNOTSUPP) => Ok(()),
                    _ => Err(err).context("acl_copy_fd fgetxattr".into()),
                };
            }
            let mut buffer = Vec::with_capacity(num_bytes as usize);
            match fgetxattr(
                fd_in,
                t_str_bytes_ptr as *const i8,
                buffer.as_mut_ptr() as *mut core::ffi::c_void,
                num_bytes as usize,
            ) {
                ret if ret < 0 => {
                    let err = io::Error::last_os_error();
                    return match err.raw_os_error() {
                        Some(libc::ENODATA) | Some(libc::EOPNOTSUPP) => Ok(()),
                        _ => Err(err).context("acl_copy_fd fgetxattr".into()),
                    };
                }
                ret => buffer.set_len(ret as usize),
            }

            if fsetxattr(
                fd_out,
                t_str_bytes_ptr as *const i8,
                buffer.as_ptr() as *mut core::ffi::c_void,
                num_bytes as usize,
                0,
            ) < 0
            {
                let io_err = io::Error::last_os_error();
                match io_err.raw_os_error() {
                    Some(libc::EPERM) => {
                        if !ignore_eperm {
                            return Err(io_err).context("acl_copy_fd fsetxattr".into());
                        }
                    }
                    Some(libc::EOPNOTSUPP) => {}
                    _ => return Err(io_err).context("acl_copy_fd fsetxattr".into()),
                }
            }
        }
    }
    Ok(())
}

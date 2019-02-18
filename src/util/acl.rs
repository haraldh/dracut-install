use libc::{fgetxattr, fsetxattr};
use std::ffi::CString;
use std::io;
use std::slice;

pub fn acl_copy_fd(fd_in: libc::c_int, fd_out: libc::c_int) -> io::Result<()> {
    let t_str = CString::new("system.posix_acl_access").unwrap();
    let t_str_bytes_ptr = t_str.as_bytes_with_nul().as_ptr();
    unsafe {
        let num_bytes = fgetxattr(
            fd_in,
            t_str_bytes_ptr as *const i8,
            0 as *mut core::ffi::c_void,
            0,
        );
        if num_bytes < 0 {
            let err = io::Error::last_os_error();
            return match err.raw_os_error() {
                Some(libc::ENOATTR) | Some(libc::ENODATA) => Ok(()),
                _ => Err(err),
            };
        }
        let mut s = ::std::mem::uninitialized();
        let buffer = slice::from_raw_parts_mut(&mut s as *mut u8, num_bytes as usize);
        let ret = fgetxattr(
            fd_in,
            t_str_bytes_ptr as *const i8,
            buffer.as_ptr() as *mut core::ffi::c_void,
            num_bytes as usize,
        );
        if ret < 0 {
            let err = io::Error::last_os_error();
            return match err.raw_os_error() {
                Some(libc::ENOATTR) | Some(libc::ENODATA) => Ok(()),
                _ => Err(err),
            };
        }
        let _ret = fsetxattr(
            fd_out,
            t_str_bytes_ptr as *const i8,
            buffer.as_ptr() as *mut core::ffi::c_void,
            num_bytes as usize,
            0,
        );
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

pub mod acl;
pub mod file;

pub use acl::acl_copy_fd;
pub use file::copy;
pub use file::ln_r;

use std::ffi::CStr;
use std::io::{self, Read};
use std::slice;

pub fn read_struct<T, R: Read>(reader: &mut R) -> io::Result<T> {
    let num_bytes = ::std::mem::size_of::<T>();
    unsafe {
        let mut s = ::std::mem::uninitialized();
        let buffer = slice::from_raw_parts_mut(&mut s as *mut T as *mut u8, num_bytes);
        match reader.read_exact(buffer) {
            Ok(()) => Ok(s),
            Err(e) => {
                ::std::mem::forget(s);
                Err(e)
            }
        }
    }
}

pub fn read_structs<T, R: Read>(reader: &mut R, num_structs: usize) -> io::Result<Vec<T>> {
    let struct_size = ::std::mem::size_of::<T>();
    let num_bytes = struct_size * num_structs;
    let mut r = Vec::<T>::with_capacity(num_structs);
    unsafe {
        let buffer = slice::from_raw_parts_mut(r.as_mut_ptr() as *mut u8, num_bytes);
        reader.read_exact(buffer)?;
        r.set_len(num_structs);
    }
    Ok(r)
}

pub struct CStrVIterator<'a> {
    slice: &'a [u8],
}

impl<'a> CStrVIterator<'a> {
    pub fn from_bytes(slice: &'a [u8]) -> Self {
        CStrVIterator { slice }
    }
}

impl<'a> Iterator for CStrVIterator<'a> {
    type Item = &'a CStr;
    fn next(&mut self) -> Option<Self::Item> {
        let s = self.slice;

        let mut index = 0usize;
        #[allow(clippy::explicit_counter_loop)]
        for elt in s {
            if *elt == 0 {
                let (a, b) = s.split_at(index + 1);
                self.slice = b;
                return unsafe { Some(CStr::from_bytes_with_nul_unchecked(a)) };
            }
            index += 1;
        }
        None
    }
}

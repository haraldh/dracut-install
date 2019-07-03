use std::io::{self, Read};
use std::slice;

pub fn read_struct<T, R: Read>(reader: &mut R) -> io::Result<T> {
    let num_bytes = ::std::mem::size_of::<T>();
    unsafe {
        let mut s = ::std::mem::MaybeUninit::uninit().assume_init();
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

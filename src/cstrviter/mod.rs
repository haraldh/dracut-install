use std::ffi::CStr;

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

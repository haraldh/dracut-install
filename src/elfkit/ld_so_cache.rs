use byteorder::{NativeEndian, ReadBytesExt};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::io::{Read, Seek, SeekFrom};
use std::mem;
use std::os::raw::c_int;
use std::os::raw::c_uint;
use std::os::unix::ffi::OsStrExt;

#[derive(Default)]
pub struct Cache<'a>(BTreeMap<&'a OsStr, Vec<&'a OsStr>>);

pub const CACHEMAGIC: &[u8] = b"ld.so-1.7.0";
pub const CACHEMAGIC_NEW: &[u8] = b"glibc-ld.so.cache";
pub const CACHE_VERSION: &[u8] = b"1.1";

impl<'a> std::ops::Deref for Cache<'a> {
    type Target = BTreeMap<&'a OsStr, Vec<&'a OsStr>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> Cache<'a> {
    pub fn read_ld_so_cache<'b: 'a>(mut string_table: &'b mut Vec<u8>) -> io::Result<Cache<'a>> {
        let mut file = File::open("/etc/ld.so.cache")?;

        let mut magic = [0u8; 12];

        file.read_exact(&mut magic)?;

        if !magic.starts_with(CACHEMAGIC) {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }

        let nlibs = file.read_u32::<NativeEndian>()?;

        let cache_file_size = (mem::size_of::<c_int>() + 2 * mem::size_of::<c_uint>()) as u64;
        let offset = file.seek(SeekFrom::Start(
            cache_file_size * u64::from(nlibs) + 12 + mem::size_of::<c_uint>() as u64,
        ))?;

        let mut magic = [0u8; 17];
        file.read_exact(&mut magic)?;
        if !magic.starts_with(CACHEMAGIC_NEW) {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }

        let mut version = [0u8; 3];
        file.read_exact(&mut version)?;
        if !version.starts_with(CACHE_VERSION) {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }

        let mut nlibs = file.read_u32::<NativeEndian>()?;
        let _len_strings = file.read_u32::<NativeEndian>()?;
        for _ in 0..5 {
            let _ = file.read_u32::<NativeEndian>()?;
        }

        let entries_pos = file.seek(SeekFrom::Current(0))?;

        let cache_file_size_new = (mem::size_of::<u32>() * 4 + mem::size_of::<u64>()) as i64;
        let offset = file.seek(SeekFrom::Current(cache_file_size_new * i64::from(nlibs)))? - offset;

        file.read_to_end(&mut string_table)?;

        file.seek(SeekFrom::Start(entries_pos))?;

        let mut cache = Cache(BTreeMap::new());

        while nlibs != 0 {
            let _flags = file.read_i32::<NativeEndian>()?;
            let key = u64::from(file.read_u32::<NativeEndian>()?) - offset;
            let value = u64::from(file.read_u32::<NativeEndian>()?) - offset;
            let _osversion = file.read_u32::<NativeEndian>()?;
            let _hwcap = file.read_u64::<NativeEndian>()?;
            let key = OsStr::from_bytes(
                string_table[key as usize..]
                    .split(|b| *b == 0u8)
                    .next()
                    .unwrap(),
            );
            let val = OsStr::from_bytes(
                string_table[value as usize..]
                    .split(|b| *b == 0u8)
                    .next()
                    .unwrap(),
            );

            cache.0.entry(key).or_insert_with(Vec::new).push(val);

            nlibs -= 1;
        }

        Ok(cache)
    }
}

use super::dl_cache::*;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::os::unix::prelude::*;
use std::path::PathBuf;

use crate::readstruct::*;

pub struct LdsoCache(BTreeMap<OsString, Vec<OsString>>);

impl std::ops::Deref for LdsoCache {
    type Target = BTreeMap<OsString, Vec<OsString>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

impl LdsoCache {
    pub fn read_ld_so_cache(sysroot: &OsStr) -> io::Result<LdsoCache> {
        let path = PathBuf::from(sysroot).join("etc/ld.so.cache");
        let mut file = File::open(path)?;
        let mut buf = Vec::<u8>::new();
        file.read_to_end(&mut buf)?;

        // Only read the well defined CACHEMAGIC_NEW structure
        let offset = match find_subsequence(&buf, CACHEMAGIC_NEW) {
            None => {
                return Err(io::Error::from(io::ErrorKind::InvalidData));
            }
            Some(o) => o,
        };

        let ld_so_cache_size = buf.len();

        let mut buf = io::Cursor::new(buf);

        buf.seek(SeekFrom::Start(offset as u64))?;

        let mut cache_file_new: CacheFileNew = read_struct(&mut buf)?;

        if cache_file_new.magic != *CACHEMAGIC_NEW {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }

        if cache_file_new.version != *CACHE_VERSION {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }

        let entries_pos = buf.seek(SeekFrom::Current(0))?;
        let byte_swap = if entries_pos as usize
            + cache_file_new.nlibs as usize * ::std::mem::size_of::<FileEntryNew>()
            + cache_file_new.len_strings as usize
            != ld_so_cache_size
        {
            // try to change the byteorder
            cache_file_new.nlibs = cache_file_new.nlibs.swap_bytes();
            cache_file_new.len_strings = cache_file_new.len_strings.swap_bytes();
            if entries_pos as usize
                + cache_file_new.nlibs as usize * ::std::mem::size_of::<FileEntryNew>()
                + cache_file_new.len_strings as usize
                != ld_so_cache_size
            {
                return Err(io::Error::from(io::ErrorKind::InvalidData));
            }
            true
        } else {
            false
        };

        let mut cache = LdsoCache(BTreeMap::new());

        let offset = buf.seek(SeekFrom::Current(
            (::std::mem::size_of::<FileEntryNew>() * cache_file_new.nlibs as usize) as i64,
        ))? as usize
            - offset;

        let mut string_table = Vec::<u8>::new();
        buf.read_to_end(&mut string_table)?;

        buf.seek(SeekFrom::Start(entries_pos as u64))?;

        let file_entries: Vec<FileEntryNew> =
            read_structs(&mut buf, cache_file_new.nlibs as usize)?;

        for mut file_entry in file_entries {
            if byte_swap {
                file_entry.key = file_entry.key.swap_bytes();
                file_entry.value = file_entry.value.swap_bytes();
            }

            let key: OsString = OsStr::from_bytes(
                string_table[(file_entry.key as usize - offset) as usize..]
                    .split(|b| *b == 0u8)
                    .next()
                    .unwrap(),
            )
            .into();
            let val: OsString = OsStr::from_bytes(
                string_table[(file_entry.value as usize - offset) as usize..]
                    .split(|b| *b == 0u8)
                    .next()
                    .unwrap(),
            )
            .into();
            cache.0.entry(key).or_insert_with(Vec::new).push(val);
            //eprintln!("{:?} => {:?}", key, val);
        }

        Ok(cache)
    }
}

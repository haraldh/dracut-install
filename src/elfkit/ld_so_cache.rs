use super::dl_cache::*;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::os::unix::ffi::OsStrExt;
use std::slice;

fn read_struct<T, R: Read>(reader: &mut R) -> io::Result<T> {
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

fn read_structs<T, R: Read>(reader: &mut R, num_structs: usize) -> io::Result<Vec<T>> {
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

pub struct LDSOCache<'a>(BTreeMap<&'a OsStr, Vec<&'a OsStr>>);

impl<'a> std::ops::Deref for LDSOCache<'a> {
    type Target = BTreeMap<&'a OsStr, Vec<&'a OsStr>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

enum FileEntries {
    Old(usize, usize),
    New(usize, usize, usize),
}

impl<'a> LDSOCache<'a> {
    pub fn read_ld_so_cache<'b: 'a>(
        mut string_table: &'b mut Vec<u8>,
    ) -> io::Result<LDSOCache<'a>> {
        let file = File::open("/etc/ld.so.cache")?;
        let mut file = BufReader::new(file);
        let mut file_entries: FileEntries;

        let cache_file: CacheFile = read_struct(&mut file)?;

        if cache_file.magic != *CACHEMAGIC {
            file.seek(SeekFrom::Start(0))?;
            let cache_file_new : CacheFileNew = read_struct(&mut file)?;

            if cache_file_new.magic != *CACHEMAGIC_NEW {
                return Err(io::Error::from(io::ErrorKind::InvalidData));
            }

            if cache_file_new.version != *CACHE_VERSION {
                return Err(io::Error::from(io::ErrorKind::InvalidData));
            }
            let entries_pos = file.seek(SeekFrom::Current(0))?;
            file_entries = FileEntries::New(entries_pos as usize, cache_file_new.nlibs as usize, 0);
        } else {
            let nlibs = cache_file.nlibs;
            let entries_pos = file.seek(SeekFrom::Current(0))?;

            let offset = file.seek(SeekFrom::Start(
                (::std::mem::size_of::<FileEntry>() as u64) * u64::from(nlibs)
                    + ::std::mem::size_of::<CacheFile>() as u64,
            ))? as usize;

            file_entries = FileEntries::Old(entries_pos as usize, cache_file.nlibs as usize);

            let cache_file_new : CacheFileNew = read_struct(&mut file)?;

            if cache_file_new.magic == *CACHEMAGIC_NEW && cache_file_new.version == *CACHE_VERSION {
                let entries_pos = file.seek(SeekFrom::Current(0))?;
                file_entries =
                    FileEntries::New(entries_pos as usize, cache_file_new.nlibs as usize, offset);
            }
        }

        let mut cache = LDSOCache(BTreeMap::new());

        match file_entries {
            FileEntries::New(entries_pos, nlibs, offset) => {
                let offset = (file.seek(SeekFrom::Current(
                    (::std::mem::size_of::<FileEntryNew>() as i64) * (nlibs as i64),
                ))? - offset as u64) as u32;

                file.read_to_end(&mut string_table)?;

                file.seek(SeekFrom::Start(entries_pos as u64))?;

                let file_entries: Vec<FileEntryNew> = read_structs(&mut file, nlibs as usize)?;

                for file_entry in file_entries {
                    let key = OsStr::from_bytes(
                        string_table[(file_entry.key - offset) as usize..]
                            .split(|b| *b == 0u8)
                            .next()
                            .unwrap(),
                    );
                    let val = OsStr::from_bytes(
                        string_table[(file_entry.value - offset) as usize..]
                            .split(|b| *b == 0u8)
                            .next()
                            .unwrap(),
                    );
                    cache.0.entry(key).or_insert_with(Vec::new).push(val);
                }
            }

            FileEntries::Old(entries_pos, nlibs) => {
                file.seek(SeekFrom::Start(
                    (::std::mem::size_of::<FileEntry>() as u64) * (nlibs as u64)
                        + entries_pos as u64,
                ))?;

                file.read_to_end(&mut string_table)?;

                file.seek(SeekFrom::Start(entries_pos as u64))?;

                let file_entries: Vec<FileEntry> = read_structs(&mut file, nlibs as usize)?;

                for file_entry in file_entries {
                    let key = OsStr::from_bytes(
                        string_table[(file_entry.key) as usize..]
                            .split(|b| *b == 0u8)
                            .next()
                            .unwrap(),
                    );
                    let val = OsStr::from_bytes(
                        string_table[(file_entry.value) as usize..]
                            .split(|b| *b == 0u8)
                            .next()
                            .unwrap(),
                    );
                    cache.0.entry(key).or_insert_with(Vec::new).push(val);
                }
            }
        }

        Ok(cache)
    }
}

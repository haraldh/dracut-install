#![allow(dead_code)]

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::io;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::RwLock;

use rayon::prelude::*;

use crate::elfkit::ld_so_cache::LDSOCache;
use crate::elfkit::ldd::Ldd;

mod elfkit;
mod util;

pub fn ldd(files: &[OsString], report_error: bool) -> Vec<OsString> {
    let mut str_table = Vec::<u8>::new();
    let sysroot = OsStr::new("/");
    let cache = LDSOCache::read_ld_so_cache(sysroot, &mut str_table).ok();

    let standard_libdirs = vec![OsString::from("/lib64/dyninst"), OsString::from("/lib64")];
    let visited = RwLock::new(BTreeSet::<OsString>::new());
    let ldd = Ldd::new(cache.as_ref(), &standard_libdirs);
    let mut _buf = Vec::<u8>::new();

    //TempDir::new_in("/var/tmp")
    let lpaths = BTreeSet::new();

    files
        .par_iter()
        .flat_map(|path| {
            let path: OsString = PathBuf::from(path)
                .canonicalize()
                .unwrap()
                .as_os_str()
                .into();

            if { visited.write().unwrap().insert(path.clone()) } {
                let mut deps = ldd.recurse(&path, &lpaths, &visited).unwrap_or_else(|e| {
                    if report_error {
                        let stderr = io::stderr();
                        let mut stderr = stderr.lock();
                        let _ = stderr.write_all(path.as_bytes());
                        let _ = stderr.write_all(b": ");
                        let _ = stderr.write_all(e.to_string().as_bytes());
                        let _ = stderr.write_all(b"\n");
                    }
                    [].to_vec()
                });
                deps.push(path);
                deps
            } else {
                [].to_vec()
            }
        })
        .collect::<Vec<_>>()
}

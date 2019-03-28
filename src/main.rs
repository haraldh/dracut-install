use std::collections::BTreeSet;
use std::env;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::io;
use std::io::Write;
use std::os;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use itertools::Itertools;
use rayon::prelude::*;

use crate::elfkit::ld_so_cache::LDSOCache;
use crate::elfkit::ldd::Ldd;

mod elfkit;
mod util;

//use tempfile::TempDir;

fn main() -> Result<(), Box<std::error::Error>> {
    let mut str_table = Vec::<u8>::new();
    let sysroot = OsStr::new("/");
    let cache = LDSOCache::read_ld_so_cache(sysroot, &mut str_table).ok();

    let standard_libdirs = vec![OsString::from("/lib64/dyninst"), OsString::from("/lib64")];
    let visited = Arc::new(RwLock::new(BTreeSet::<OsString>::new()));
    let ldd = Ldd::new(cache.as_ref(), &standard_libdirs);
    let mut _buf = Vec::<u8>::new();
    let mut destrootdir = ::std::env::var_os("DESTROOTDIR").expect("DESTROOTDIR is unset");
    let /* mut */ destpath = PathBuf::from(&destrootdir);

    //TempDir::new_in("/var/tmp")
    let lpaths = BTreeSet::new();

    for i in env::args_os()
        .skip(1)
        .collect_vec()
        .par_iter()
        .cloned()
        .flat_map(|ref path| {
            let path: OsString = PathBuf::from(path)
                .canonicalize()
                .unwrap()
                .as_os_str()
                .into();

            if { visited.write().unwrap().insert(path.clone()) } {
                let mut deps = ldd
                    .recurse(&path, &lpaths, &visited)
                    .unwrap_or_else(|e| {
                        let stderr = io::stderr();
                        let mut stderr = stderr.lock();
                        let _ = stderr.write_all(path.as_bytes());
                        let _ = stderr.write_all(b": ");
                        let _ = stderr.write_all(e.to_string().as_bytes());
                        let _ = stderr.write_all(b"\n");

                        Vec::<OsString>::new()
                    });
                deps.push(path);
                deps
            } else {
                Vec::<OsString>::new()
            }
        })
        .collect::<Vec<_>>()
    {
        println!(
            "cp {} {}",
            i.to_string_lossy(),
            destpath.join(&i).to_string_lossy()
        );
    }
    Ok(())
}

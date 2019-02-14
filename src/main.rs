mod elfkit;

use crate::elfkit::ld_so_cache::LDSOCache;
use crate::elfkit::ldd::Ldd;

use std::env;
use std::ffi::OsString;
use std::io;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::collections::BTreeSet;

fn main() -> Result<(), Box<std::error::Error>> {
    let stdout = io::stdout();
    let mut str_table = Vec::<u8>::new();
    let cache = LDSOCache::read_ld_so_cache(&mut str_table)
        .map_err(|e| {
            eprintln!("Cannot read `/etc/ld.so.conf`: {}", e);
            std::process::exit(1);
        })
        .unwrap();
    let mut stdout = stdout.lock();


    let standard_libdirs = vec![
        OsString::from("/lib64/dyninst"),
        OsString::from("/lib64"),
    ];
    let mut visited = BTreeSet::<OsString>::new();
    let mut ldd = Ldd::new(&cache, &standard_libdirs);
    for i in env::args_os().skip(1).flat_map(|ref path| {
        let path: OsString = PathBuf::from(path)
            .canonicalize()
            .unwrap()
            .as_os_str()
            .into();
        //        let stderr = io::stderr();
        //        let mut stderr = stderr.lock();
        //        let _ = stderr.write_all(path.as_bytes());
        //        let _ = stderr.write_all(b":\n");
        ldd.recurse(&path, &BTreeSet::new(), &mut visited ).unwrap_or_else(|e| {
            let stderr = io::stderr();
            let mut stderr = stderr.lock();
            let _ = stderr.write_all(path.as_bytes());
            let _ = stderr.write_all(b": ");
            let _ = stderr.write_all(e.to_string().as_bytes());
            let _ = stderr.write_all(b"\n");
            Vec::<OsString>::new()
        })
    }) {
        stdout.write_all(i.as_bytes())?;
        stdout.write_all(&[b'\n'])?;
    }
    Ok(())
}

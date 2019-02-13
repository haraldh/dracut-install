mod elfkit;

use crate::elfkit::ld_so_cache::Cache;
use crate::elfkit::ldd::Ldd;

use std::env;
use std::ffi::OsString;
use std::io;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

fn main() -> Result<(), Box<std::error::Error>> {
    let stdout = io::stdout();
    let mut str_table = Vec::<u8>::new();
    let cache = Cache::read_ld_so_cache(&mut str_table)
        .map_err(|e| {
            eprintln!("Cannot read `/etc/ld.so.conf`: {}", e);
            std::process::exit(1);
        })
        .unwrap();
    let mut stdout = stdout.lock();

    let mut ldd = Ldd::new(&cache);
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
        ldd.recurse(&path, Vec::new()).unwrap_or_else(|e| {
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

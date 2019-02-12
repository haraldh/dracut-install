mod elfkit;

use crate::elfkit::Elf;
use byteorder::{NativeEndian, ReadBytesExt};
use std::collections::HashSet;
use std::fs::File;
use std::io;
use std::io::{Read, Seek, SeekFrom};
use std::mem;
use std::os::raw::c_int;
use std::os::raw::c_uint;
use std::path::Path;

pub struct FileEntryNew {
    /* This is 1 for an ELF library.  */
    pub flags: i32,
    /* String table indices.  */
    pub key: String,
    /* String table indices.  */
    pub value: String,
    /* Required OS version.	 */
    pub osversion: u32,
    /* Hwcap entry.	 */
    pub hwcap: u64,
}

pub const CACHEMAGIC: &[u8] = b"ld.so-1.7.0";
pub const CACHEMAGIC_NEW: &[u8] = b"glibc-ld.so.cache";
pub const CACHE_VERSION: &[u8] = b"1.1";

pub fn read_ld_so_cache(sysroot: &str) -> io::Result<Vec<FileEntryNew>> {
    let mut file = File::open(Path::new(sysroot).join("/etc/ld.so.cache"))?;

    let mut magic = [0; 12];

    file.read_exact(&mut magic)?;

    if !magic.starts_with(CACHEMAGIC) {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }

    let nlibs = file.read_u32::<NativeEndian>()?;

    let cache_file_size = (mem::size_of::<c_int>() + 2 * mem::size_of::<c_uint>()) as u64;
    let offset = file.seek(SeekFrom::Start(
        cache_file_size * u64::from(nlibs) + 12 + mem::size_of::<c_uint>() as u64,
    ))?;

    let mut magic = [0; 17];
    file.read_exact(&mut magic)?;
    if !magic.starts_with(CACHEMAGIC_NEW) {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }

    let mut version = [0; 3];
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

    let mut string_table = Vec::new();
    file.read_to_end(&mut string_table)?;

    file.seek(SeekFrom::Start(entries_pos))?;
    let mut file_entries = Vec::new();

    while nlibs != 0 {
        let flags = file.read_i32::<NativeEndian>()?;
        let key = u64::from(file.read_u32::<NativeEndian>()?) - offset;
        let value = u64::from(file.read_u32::<NativeEndian>()?) - offset;
        let osversion = file.read_u32::<NativeEndian>()?;
        let hwcap = file.read_u64::<NativeEndian>()?;

        let file_entry_new = FileEntryNew {
            flags,
            key: String::from_utf8_lossy(
                string_table[key as usize..]
                    .split(|b| *b == 0u8)
                    .next()
                    .unwrap(),
            )
            .into(),
            value: String::from_utf8_lossy(
                string_table[value as usize..]
                    .split(|b| *b == 0u8)
                    .next()
                    .unwrap(),
            )
            .into(),
            osversion,
            hwcap,
        };
        file_entries.push(file_entry_new);

        nlibs -= 1;
    }

    Ok(file_entries)
}

struct Ldd {
    sysroot: String,
    file_entries: Vec<FileEntryNew>,
    visited: HashSet<String>,
}

fn join_paths(a: &str, b: &str) -> String {
    if b.is_empty() {
        return String::from(a);
    }
    let mut a = String::from(a);
    if a.is_empty() {
        return a;
    }
    if !a.ends_with('/') {
        a.push('/');
    }

    if b.starts_with('/') {
        return a + &b[1..];
    }
    a + b
}

impl Ldd {
    fn recurse(
        &mut self,
        path: &str,
        mut lpaths: Vec<String>,
    ) -> Result<(), Box<std::error::Error>> {
        let mut f = File::open(path)?;
        let mut elf = match Elf::from_reader(&mut f) {
            Ok(e) => e,
            Err(elfkit::Error::InvalidMagic) => {
                return Err("not a dynamic executable".into());
            }
            Err(e) => {
                return Err(format!("{:#?}", e).into());
            }
        };

        let mut deps = Vec::new();
        for shndx in 0..elf.sections.len() {
            if elf.sections[shndx].header.shtype == elfkit::types::SectionType::DYNAMIC {
                elf.load(shndx, &mut f).unwrap();
                let dynamic = elf.sections[shndx].content.as_dynamic().unwrap();

                for r#dyn in dynamic.iter() {
                    //eprintln!("dyn: {:#?}", dyn);
                    if r#dyn.dhtype == elfkit::types::DynamicType::RPATH {
                        if let elfkit::dynamic::DynamicContent::String(ref name) = r#dyn.content {
                            //eprintln!("RPATH: {:#?}", name);
                            for n in name.0.split(|e| *e == b':') {
                                lpaths.push(join_paths(
                                    &self.sysroot,
                                    &String::from_utf8_lossy(&n).into_owned(),
                                ))
                            }
                        }
                    }
                    if r#dyn.dhtype == elfkit::types::DynamicType::RUNPATH {
                        if let elfkit::dynamic::DynamicContent::String(ref name) = r#dyn.content {
                            //eprintln!("RUNPATH: {:#?}", name);
                            for n in name.0.split(|e| *e == b':') {
                                lpaths.push(join_paths(
                                    &self.sysroot,
                                    &String::from_utf8_lossy(&n).into_owned(),
                                ))
                            }
                        }
                    }
                    if r#dyn.dhtype == elfkit::types::DynamicType::NEEDED {
                        if let elfkit::dynamic::DynamicContent::String(ref name) = r#dyn.content {
                            deps.push(String::from_utf8_lossy(&name.0).into_owned());
                        }
                    }
                }
            }
        }

        for dep in &mut deps {
            let mut found = false;

            for lpath in lpaths.iter() {
                let joined = Path::new(lpath).join(dep.clone());
                //let joined = join_paths(&lpath, &dep);
                //let joined = Path::new(&joined);
                if joined.exists() {
                    *dep = joined.to_string_lossy().into_owned();
                    found = true;
                    break;
                }
            }

            if found {
                if self.visited.insert(dep.clone()) {
                    println!("{}", dep);
                    let _ = self.recurse(dep, lpaths.clone());
                }
            } else {
                let cache: Vec<String> = self
                    .file_entries
                    .iter()
                    .filter_map(|k| {
                        //eprintln!("{} == {}", k.key, *dep);
                        if k.key == *dep {
                            Some(k.value.clone())
                        } else {
                            None
                        }
                    })
                    .collect();

                if !cache.is_empty() {
                    for l in cache {
                        if self.visited.insert(l.clone()) {
                            println!("{}", l);
                            let _ = self.recurse(&l, lpaths.clone());
                        }
                    }
                } else {
                    return Err(format!("unable to find dependency {} in {:?}", dep, lpaths).into());
                }
            }
        }
        Ok(())
    }
}

fn main() -> Result<(), Box<std::error::Error>> {
    let matches = clap::App::new("elfkit-ldd")
        .setting(clap::AppSettings::ArgRequiredElseHelp)
        .setting(clap::AppSettings::UnifiedHelpMessage)
        .setting(clap::AppSettings::DisableHelpSubcommand)
        .version("0.5")
        .arg(
            clap::Arg::with_name("file")
                .required(true)
                .help("path to binary to inspect")
                .takes_value(true)
                .multiple(true),
        )
        .arg(
            clap::Arg::with_name("library-path")
                .short("L")
                .long("library-path")
                .takes_value(true)
                .multiple(true)
                .help("library lookup path, ignores $SYSROOT/etc/ld.so.conf"),
        )
        .arg(
            clap::Arg::with_name("sysroot")
                .short("R")
                .long("sysroot")
                .takes_value(true)
                .help("specify sysroot to look up dependencies in, instead of /"),
        )
        .get_matches();

    let sysroot = matches.value_of("sysroot").unwrap_or("/").to_owned();

    let file_entries = read_ld_so_cache(&sysroot)?;

    let mut ldd = Ldd {
        sysroot,
        file_entries,
        visited: HashSet::new(),
    };

    for path in matches.values_of("file").unwrap() {
        let _ = ldd
            .recurse(&path, Vec::new())
            .map_err(|e| eprintln!("{}: {}", path, e));
    }
    Ok(())
}

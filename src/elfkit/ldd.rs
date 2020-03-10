use std::ffi::OsStr;
use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::os::unix::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use hashbrown::HashMap;
use hashbrown::HashSet;

use crate::elfkit::{self, ld_so_cache::LDSOCache, Elf};

fn replace_slice<T: Copy>(buf: &[T], from: &[T], to: &[T]) -> Vec<T>
where
    T: Clone + PartialEq,
{
    if buf.len() < from.len() {
        return Vec::from(buf);
    }

    let mut res: Vec<T> = Vec::new();
    let mut i = 0;
    while i <= buf.len() - from.len() {
        if buf[i..].starts_with(from) {
            res.extend_from_slice(to);
            i += from.len();
        } else {
            res.push(buf[i]);
            i += 1;
        }
    }

    if i < buf.len() {
        res.extend_from_slice(&buf[i..buf.len()]);
    }
    res
}

pub struct Ldd<'a> {
    pub ld_so_cache: Option<&'a LDSOCache>,
    pub default_libdir: &'a [OsString],
    pub canon_cache: RwLock<HashMap<OsString, OsString>>,
    pub dest_path: OsString,
}

impl<'a> Ldd<'a> {
    pub fn new(
        ld_so_cache: Option<&'a LDSOCache>,
        slpath: &'a [OsString],
        dest_path: &PathBuf,
    ) -> Ldd<'a> {
        Ldd {
            ld_so_cache,
            default_libdir: slpath,
            canon_cache: RwLock::new(HashMap::new()),
            dest_path: OsString::from(dest_path.as_os_str()),
        }
    }
    pub fn recurse(
        &self,
        handle: dynqueue::DynQueueHandle<
            (OsString, HashSet<OsString>),
            RwLock<Vec<(OsString, HashSet<OsString>)>>,
        >,
        path: &OsStr,
        lpaths: &HashSet<OsString>,
        visited: &RwLock<HashSet<OsString>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut lpaths = lpaths.clone();
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

        let mut deps: Vec<OsString> = Vec::new();
        for shndx in 0..elf.sections.len() {
            if elf.sections[shndx].header.shtype == elfkit::types::SectionType::DYNAMIC {
                elf.load(shndx, &mut f).unwrap();
                let dynamic = elf.sections[shndx].content.as_dynamic().unwrap();

                for dyn_entry in dynamic.iter() {
                    if dyn_entry.dhtype == elfkit::types::DynamicType::RPATH {
                        if let elfkit::dynamic::DynamicContent::String(ref name) = dyn_entry.content
                        {
                            name.0.split(|e| *e == b':').for_each(|n| {
                                let n = replace_slice(
                                    &n,
                                    b"$ORIGIN",
                                    PathBuf::from(path).parent().unwrap().as_os_str().as_bytes(),
                                );

                                lpaths.insert(OsString::from(OsStr::from_bytes(&n)));
                            });
                        }
                    }
                    if dyn_entry.dhtype == elfkit::types::DynamicType::RUNPATH {
                        if let elfkit::dynamic::DynamicContent::String(ref name) = dyn_entry.content
                        {
                            name.0.split(|e| *e == b':').for_each(|n| {
                                let n = replace_slice(
                                    &n,
                                    b"$ORIGIN",
                                    PathBuf::from(path).parent().unwrap().as_os_str().as_bytes(),
                                );

                                lpaths.insert(OsString::from(OsStr::from_bytes(&n)));
                            });
                        }
                    }
                    if dyn_entry.dhtype == elfkit::types::DynamicType::NEEDED {
                        if let elfkit::dynamic::DynamicContent::String(ref name) = dyn_entry.content
                        {
                            deps.push(OsStr::from_bytes(&name.0).into());
                        }
                    }
                }
            }
        }

        'outer: for dep in deps {
            //eprintln!("Search for {:#?}", dep);
            for lpath in lpaths.iter() {
                let joined = PathBuf::from(lpath).join(&dep);
                let joined = self.canonicalize_dir(&joined).unwrap_or(joined);
                //eprintln!("Checking {:#?}", joined);

                if visited.write().unwrap().insert(joined.clone().into()) {
                    let mut dest = self.dest_path.clone();
                    dest.push(joined.as_os_str());
                    let dest = PathBuf::from(dest);
                    if joined.exists() && !dest.exists() {
                        handle.enqueue((joined.into(), lpaths.clone()));
                        continue 'outer;
                    }
                } else {
                    continue 'outer;
                }
            }

            if let Some(ld_so_cache) = self.ld_so_cache {
                if let Some(vals) = ld_so_cache.get(dep.as_os_str()) {
                    for f in vals {
                        //eprintln!("LD_SO_CACHE Found {:#?}", val);
                        let joined = PathBuf::from(f);
                        let joined = self.canonicalize_dir(&joined).unwrap_or(joined);
                        //eprintln!("Checking {:#?}", joined);

                        if visited.write().unwrap().insert(joined.clone().into()) {
                            let mut dest = self.dest_path.clone();
                            dest.push(joined.as_os_str());
                            let dest = PathBuf::from(dest);

                            if joined.exists() && !dest.exists() {
                                handle.enqueue((joined.into(), lpaths.clone()));
                            }
                        }
                    }
                    continue 'outer;
                }
            }

            for lpath in self.default_libdir.iter() {
                let joined = PathBuf::from(lpath).join(&dep);
                let joined = self.canonicalize_dir(&joined).unwrap_or(joined);

                //eprintln!("Checking {:#?}", joined);

                if visited.write().unwrap().insert(joined.clone().into()) {
                    let mut dest = self.dest_path.clone();
                    dest.push(&joined);
                    let dest = PathBuf::from(dest);
                    if joined.exists() && !dest.exists() {
                        //eprintln!("Standard LIBPATH Found {:#?}", joined);
                        handle.enqueue((joined.into(), lpaths.clone()));
                        continue 'outer;
                    }
                } else {
                    continue 'outer;
                }
            }

            return Err(format!("unable to find dependency {:#?} in {:?}", dep, lpaths).into());
        }
        //eprintln!("{:#?}", out);
        Ok(())
    }

    #[inline]
    pub fn canonicalize_dir(&self, path: &Path) -> std::result::Result<PathBuf, ()> {
        let source_filename = path.file_name().ok_or(())?;
        let dirname = path.parent().ok_or(())?;
        let mut canon_dirname = self.canonicalize(dirname).map_err(|_| ())?;
        canon_dirname.push(source_filename);
        Ok(canon_dirname)
    }

    #[inline]
    pub fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        {
            if let Some(val) = self.canon_cache.read().unwrap().get(path.as_os_str()) {
                return Ok(PathBuf::from(val));
            }
        }
        let val = path.canonicalize()?;
        {
            self.canon_cache
                .write()
                .unwrap()
                .insert(path.as_os_str().into(), val.as_os_str().into());
        }
        Ok(val)
    }
}

#[cfg(test)]
mod test {
    use super::replace_slice;

    #[test]
    fn test_replace_slice() {
        assert_eq!(replace_slice(b"test", b"$ORIGIN", b"TEST"), b"test");
        assert_eq!(replace_slice(b"$ORIGIN", b"$ORIGIN", b"TEST"), b"TEST");
        assert_eq!(replace_slice(b"/$ORIGIN/", b"$ORIGIN", b"TEST"), b"/TEST/");
        assert_eq!(
            replace_slice(b"/_ORIGIN/", b"$ORIGIN", b"TEST"),
            b"/_ORIGIN/"
        );
        assert_eq!(
            replace_slice(b"/_ORIGIN//", b"$ORIGIN", b"TEST"),
            b"/_ORIGIN//"
        );
    }
}

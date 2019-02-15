use crate::elfkit::{self, ld_so_cache::LDSOCache, Elf};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

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

pub struct Ldd<'a, 'b: 'a> {
    pub ld_so_cache: &'a LDSOCache<'b>,
    pub default_libdir: &'a [OsString],
    pub canon_cache: BTreeMap<OsString, OsString>,
}

impl<'a, 'b: 'a> Ldd<'a, 'b> {
    pub fn new(ld_so_cache: &'a LDSOCache<'b>, slpath: &'a [OsString]) -> Ldd<'a, 'b> {
        Ldd {
            ld_so_cache,
            default_libdir: slpath,
            canon_cache: BTreeMap::new(),
        }
    }
    pub fn recurse(
        &mut self,
        path: &OsStr,
        lpaths: &BTreeSet<OsString>,
        visited: &mut BTreeSet<OsString>,
    ) -> Result<Vec<OsString>, Box<std::error::Error>> {
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

        let mut out: Vec<OsString> = Vec::new();

        'outer: for dep in deps {
            //eprintln!("Search for {:#?}", dep);
            for lpath in lpaths.iter() {
                let joined = PathBuf::from(lpath).join(&dep);
                //eprintln!("Checking {:#?}", joined);

                let f = joined.as_os_str();
                if visited.insert(f.into()) {
                    if joined.exists() {
                        //eprintln!("Found {:#?}", joined);
                        joined
                            .parent()
                            .ok_or_else(|| {
                                ::std::io::Error::from(::std::io::ErrorKind::InvalidData)
                            })
                            .and_then(|p| self.canonicalize(p))
                            .and_then(|v| {
                                let v = v.join(joined.file_name().unwrap());
                                let t = v.as_os_str();
                                if t == f || visited.insert(t.into()) {
                                    out.push(t.into());
                                }
                                Ok(())
                            })
                            .unwrap_or_else(|_| {
                                out.push(f.into());
                            });
                        out.append(&mut self.recurse(f, &lpaths, visited)?);
                        continue 'outer;
                    }
                } else {
                    continue 'outer;
                }
            }

            if let Some(vals) = self.ld_so_cache.get(dep.as_os_str()) {
                for f in vals {
                    //eprintln!("LD_SO_CACHE Found {:#?}", val);
                    if visited.insert(OsString::from(f)) {
                        let joined = PathBuf::from(f);
                        joined
                            .parent()
                            .ok_or_else(|| {
                                ::std::io::Error::from(::std::io::ErrorKind::InvalidData)
                            })
                            .and_then(|p| self.canonicalize(p))
                            .and_then(|v| {
                                let v = v.join(joined.file_name().unwrap());
                                let t = v.as_os_str();
                                if t == *f || visited.insert(t.into()) {
                                    out.push(t.into());
                                }
                                Ok(())
                            })
                            .unwrap_or_else(|_| {
                                out.push(f.into());
                            });
                        out.append(&mut self.recurse(f, &lpaths, visited)?);
                    }
                }
                continue 'outer;
            }

            for lpath in self.default_libdir.iter() {
                let joined = PathBuf::from(lpath).join(&dep);
                //eprintln!("Checking {:#?}", joined);
                //eprintln!("Found {:#?}", joined);

                let f = joined.as_os_str();
                if visited.insert(f.into()) {
                    if joined.exists() {
                        //eprintln!("Standard LIBPATH Found {:#?}", joined);
                        joined
                            .parent()
                            .ok_or_else(|| {
                                ::std::io::Error::from(::std::io::ErrorKind::InvalidData)
                            })
                            .and_then(|p| self.canonicalize(p))
                            .and_then(|v| {
                                let v = v.join(joined.file_name().unwrap());
                                let t = v.as_os_str();
                                if t == f || visited.insert(t.into()) {
                                    out.push(t.into());
                                }
                                Ok(())
                            })
                            .unwrap_or_else(|_| {
                                out.push(f.into());
                            });
                        out.append(&mut self.recurse(f, &lpaths, visited)?);
                        continue 'outer;
                    }
                } else {
                    continue 'outer;
                }
            }

            return Err(format!("unable to find dependency {:#?} in {:?}", dep, lpaths).into());
        }
        //eprintln!("{:#?}", out);
        Ok(out)
    }

    pub fn canonicalize(&mut self, path: &Path) -> io::Result<PathBuf> {
        //path.canonicalize()

        if let Some(val) = self.canon_cache.get(path.as_os_str()) {
            Ok(PathBuf::from(val))
        } else {
            let val = path.canonicalize()?;
            self.canon_cache
                .insert(path.as_os_str().into(), val.as_os_str().into());
            Ok(val)
        }
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

use crate::elfkit::{self, ld_so_cache::Cache, Elf};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fs::File;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

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
    pub visited: BTreeSet<OsString>,
    pub cache: &'a Cache<'b>,
    pub slpath: &'a Vec<OsString>,
}

impl<'a, 'b: 'a> Ldd<'a, 'b> {
    pub fn new(cache: &'a Cache<'b>, slpath: &'a Vec<OsString>) -> Ldd<'a, 'b> {
        Ldd {
            visited: BTreeSet::new(),
            cache,
            slpath,
        }
    }
    pub fn recurse(
        &mut self,
        path: &OsStr,
        lpaths: &Vec<OsString>,
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
                            lpaths.splice(
                                0..0,
                                name.0.split(|e| *e == b':').map(|n| {
                                    let n = replace_slice(
                                        &n,
                                        b"$ORIGIN",
                                        PathBuf::from(path)
                                            .parent()
                                            .unwrap()
                                            .as_os_str()
                                            .as_bytes(),
                                    );
                                    OsString::from(OsStr::from_bytes(&n))
                                }),
                            );
                        }
                    }
                    if dyn_entry.dhtype == elfkit::types::DynamicType::RUNPATH {
                        if let elfkit::dynamic::DynamicContent::String(ref name) = dyn_entry.content
                        {
                            lpaths.splice(
                                0..0,
                                name.0.split(|e| *e == b':').map(|n| {
                                    let n = replace_slice(
                                        &n,
                                        b"$ORIGIN",
                                        PathBuf::from(path)
                                            .parent()
                                            .unwrap()
                                            .as_os_str()
                                            .as_bytes(),
                                    );
                                    OsString::from(OsStr::from_bytes(&n))
                                }),
                            );
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
                if joined.exists() {
                    //eprintln!("Found {:#?}", joined);

                    let f = joined.as_os_str();
                    if self.visited.insert(f.into()) {
                        out.push(f.into());
                        out.append(&mut self.recurse(f.into(), &lpaths)?);
                    }
                    continue 'outer;
                }
            }

            if let Some(vals) = self.cache.get(dep.as_os_str()) {
                for val in vals {
                    if self.visited.insert(OsString::from(val)) {
                        out.push(val.into());
                        out.append(&mut self.recurse(val, &lpaths)?);
                    }
                }
                continue 'outer;
            }


            for lpath in self.slpath.iter() {
                let joined = PathBuf::from(lpath).join(&dep);
                //eprintln!("Checking {:#?}", joined);
                if joined.exists() {
                    //eprintln!("Found {:#?}", joined);

                    let f = joined.as_os_str();
                    if self.visited.insert(f.into()) {
                        out.push(f.into());
                        out.append(&mut self.recurse(f.into(), &lpaths)?);
                    }
                    continue 'outer;
                }
            }

            return Err(format!("unable to find dependency {:#?} in {:?}", dep, lpaths).into());
        }
        //eprintln!("{:#?}", out);
        Ok(out)
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

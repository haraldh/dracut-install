#![allow(dead_code)]

use std::ffi::OsStr;
use std::ffi::OsString;
use std::io;
use std::io::Write;
use std::os::unix::prelude::*;
use std::path::PathBuf;
use std::sync::RwLock;

use hashbrown::HashSet;
use rayon::prelude::*;

use slog::{debug, o, Level, Logger};
use walkdir::WalkDir;

use chainerror::prelude::v1::*;

use regex::bytes::Regex;

use crate::elfkit::ld_so_cache::LDSOCache;
use crate::elfkit::ldd::Ldd;
use crate::file::{canonicalize_dir, clone_path};
pub use crate::modules::modalias_list;
use dynqueue::IntoDynQueue;

mod acl;
mod cstrviter;
mod elfkit;
mod file;
mod modules;
mod readstruct;

pub type ResultSend<T> = std::result::Result<T, Box<dyn std::error::Error + 'static + Send>>;

pub struct RunContext {
    pub hmac: bool,
    pub createdir: bool,
    pub optional: bool,
    pub silent: bool,
    pub all: bool,
    pub module: bool,
    pub modalias: bool,
    pub resolvelazy: bool,
    pub resolvedeps: bool,
    pub hostonly: bool,
    pub loglevel: Level,
    pub destrootdir: PathBuf,
    pub kerneldir: Option<OsString>,
    pub logdir: Option<OsString>,
    pub logger: Logger,
    pub mod_filter_path: Option<Regex>,
    pub mod_filter_nopath: Option<Regex>,
    pub mod_filter_symbol: Option<Regex>,
    pub mod_filter_nosymbol: Option<Regex>,
    pub mod_filter_noname: Option<Regex>,
    pub firmwaredirs: Vec<OsString>,
    pub pathdirs: Vec<OsString>,
}

impl Default for RunContext {
    fn default() -> Self {
        RunContext {
            hmac: false,
            createdir: false,
            optional: false,
            silent: false,
            all: false,
            module: false,
            modalias: false,
            resolvelazy: false,
            resolvedeps: false,
            hostonly: false,
            loglevel: Level::Critical,
            destrootdir: Default::default(),
            kerneldir: None,
            logdir: None,
            logger: slog::Logger::root(slog::Discard, o!()),
            mod_filter_path: None,
            mod_filter_nopath: None,
            mod_filter_symbol: None,
            mod_filter_nosymbol: None,
            mod_filter_noname: None,
            firmwaredirs: vec![],
            pathdirs: vec![],
        }
    }
}

pub fn ldd(files: &[OsString], report_error: bool, dest_path: &PathBuf) -> Vec<OsString> {
    let sysroot = OsStr::new("/");
    let cache = LDSOCache::read_ld_so_cache(sysroot).ok();

    let standard_libdirs = vec![OsString::from("/lib64/dyninst"), OsString::from("/lib64")];
    let visited = RwLock::new(HashSet::<OsString>::new());
    let ldd = Ldd::new(cache.as_ref(), &standard_libdirs, dest_path);
    let mut _buf = Vec::<u8>::new();

    //let lpaths = HashSet::new();

    let dest = OsString::from(dest_path.as_os_str());

    let filequeue = Vec::from(
        files
            .to_vec()
            .drain(..)
            .map(|path| {
                canonicalize_dir(PathBuf::from(path))
                    .unwrap()
                    .as_os_str()
                    .to_os_string()
            })
            .map(|path| {
                visited.write().unwrap().insert(path.clone());
                (path, HashSet::new())
            })
            .collect::<Vec<_>>(),
    )
    .into_dyn_queue();

    filequeue
        .into_par_iter()
        .filter_map(|(handle, (path, lpaths))| {
            let mut dest = dest.clone();
            dest.push(path.as_os_str());
            let dest = PathBuf::from(dest);
            if !dest.exists() {
                ldd.recurse(handle, &path, &lpaths, &visited)
                    .unwrap_or_else(|e| {
                        if report_error {
                            let stderr = io::stderr();
                            let mut stderr = stderr.lock();
                            let _ = stderr.write_all(path.as_bytes());
                            let _ = stderr.write_all(b": ");
                            let _ = stderr.write_all(e.to_string().as_bytes());
                            let _ = stderr.write_all(b"\n");
                        }
                    });
                Some(path)
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
}

pub fn install_files_ldd(
    ctx: &mut RunContext,
    files: &[OsString],
) -> Result<(), Box<dyn std::error::Error + 'static + Send + Sync>> {
    debug!(ctx.logger, "Path = {:#?}", ctx.pathdirs);
    debug!(ctx.logger, "FirmwareDirs = {:#?}", ctx.firmwaredirs);
    debug!(ctx.logger, "KernelDir = {:#?}", ctx.kerneldir);

    let res = ldd(&files, true, &ctx.destrootdir);
    debug!(ctx.logger, "install {:#?}", res);
    install_files(ctx, &res)
}

pub fn install_files(
    ctx: &mut RunContext,
    files: &[OsString],
) -> Result<(), Box<dyn std::error::Error + 'static + Send + Sync>> {
    for i in files {
        clone_path(&PathBuf::from(i), &ctx.destrootdir)?;
    }

    Ok(())
}

//noinspection RsUnresolvedReference,RsUnresolvedReference
pub fn install_modules(
    ctx: &mut RunContext,
    module_args: &[OsString],
) -> Result<(), Box<dyn std::error::Error + 'static + Send + Sync>> {
    let visited = RwLock::new(HashSet::<OsString>::new());

    let kmod_ctx = kmod::Context::new_with(ctx.kerneldir.as_ref().map(OsString::as_os_str), None)
        .context("kmod::Context::new_with")?;

    let (module_iterators, errors): (Vec<_>, Vec<_>) = module_args
        .iter()
        .flat_map(|module| {
            if module.as_bytes().starts_with(b"/") {
                debug!(ctx.logger, "by file path");
                let m = kmod_ctx
                    .module_new_from_path(module.to_str().unwrap())
                    .unwrap();
                if let Some(name) = m.name() {
                    return vec![kmod_ctx.module_new_from_lookup(&OsString::from(name))];
                } else {
                    return vec![Err(kmod::ErrorKind::Errno(kmod::Errno(42)).into())];
                }
            } else if module.as_bytes().starts_with(b"=") {
                let (_, b) = module.as_bytes().split_at(1);

                let driver_dir = OsString::from_vec(b.to_vec());

                let mut dirname = PathBuf::from(kmod_ctx.dirname());

                dirname.push("kernel");

                if !driver_dir.is_empty() {
                    dirname.push(driver_dir);
                }

                debug!(ctx.logger, "driver_dir {}", dirname.to_str().unwrap());

                WalkDir::new(dirname)
                    .into_iter()
                    .filter_map(std::result::Result::<_, _>::ok)
                    .filter(|e| e.path().is_file())
                    .map(|e| {
                        kmod_ctx
                            .module_new_from_path(e.path().to_str().unwrap())
                            .and_then(|m| {
                                m.name()
                                    .ok_or_else(|| kmod::ErrorKind::Errno(kmod::Errno(42)).into())
                                    .map(OsString::from)
                            })
                            .and_then(|name| kmod_ctx.module_new_from_lookup(&name))
                    })
                    .collect::<Vec<_>>()
            } else {
                debug!(ctx.logger, "by name {}", module.to_str().unwrap());

                if let Some(module) = PathBuf::from(module).file_stem() {
                    debug!(ctx.logger, "file stem {}", module.to_str().unwrap());
                    if let Some(module) = PathBuf::from(module).file_stem() {
                        debug!(ctx.logger, "file stem {}", module.to_str().unwrap());
                        vec![kmod_ctx.module_new_from_lookup(module)]
                    } else {
                        vec![kmod_ctx.module_new_from_lookup(module)]
                    }
                } else {
                    vec![kmod_ctx.module_new_from_lookup(module)]
                }
            }
        })
        .partition(kmod::Result::is_ok);

    let errors: Vec<_> = errors.into_iter().map(kmod::Result::unwrap_err).collect();

    if !errors.is_empty() {
        for e in errors {
            eprintln!("Module Error: {:?}", e);
        }
        return Err("Module errors".into());
    }

    let modules: Vec<_> = module_iterators
        .into_iter()
        .map(kmod::Result::unwrap)
        .flat_map(|it| it.collect::<Vec<_>>())
        .collect();

    let install_errors: Vec<_> = modules
        .into_iter()
        .map(|m| install_module(ctx, &kmod_ctx, &m, &visited, true))
        .filter(ChainResult::is_err)
        .map(ChainResult::unwrap_err)
        .collect();

    if !install_errors.is_empty() {
        for e in install_errors {
            eprintln!("{:?}", e);
        }
        return Err("Module errors".into());
    }

    let files = visited.write().unwrap().drain().collect::<Vec<_>>();

    install_files(ctx, &files)
}

derive_str_context!(InstallModuleError);

fn filter_module_name_path_symbols(ctx: &RunContext, module: &kmod::Module, path: &OsStr) -> bool {
    if let Some(name) = module.name() {
        if let Some(ref r) = ctx.mod_filter_noname {
            if r.is_match(name.as_bytes()) {
                return false;
            }
        }
    }

    if let Some(ref r) = ctx.mod_filter_nopath {
        if r.is_match(path.as_bytes()) {
            return false;
        }
    }

    if let Some(ref r) = ctx.mod_filter_path {
        if !r.is_match(path.as_bytes()) {
            return false;
        }
    }

    if ctx.mod_filter_nosymbol.is_none() && ctx.mod_filter_symbol.is_none() {
        return true;
    }

    let sit = match module.dependency_symbols() {
        Err(e) => {
            slog::warn!(
                ctx.logger,
                "Error getting dependency symbols for {:?}: {}",
                path,
                e
            );
            return true;
        }
        Ok(sit) => sit,
    };

    for symbol in sit {
        if let Some(ref r) = ctx.mod_filter_nosymbol {
            if r.is_match(symbol.as_bytes()) {
                return false;
            }
        }

        if let Some(ref r) = ctx.mod_filter_symbol {
            if r.is_match(symbol.as_bytes()) {
                return true;
            }
        }
    }

    false
}

fn install_module(
    ctx: &RunContext,
    kmod_ctx: &kmod::Context,
    module: &kmod::Module,
    visited: &RwLock<HashSet<OsString>>,
    filter: bool,
) -> ChainResult<(), InstallModuleError> {
    debug!(
        ctx.logger,
        "handling module <{:?}>",
        module.name().unwrap_or_default()
    );

    let path = match module.path() {
        Some(p) => p,
        None => {
            // FIXME: Error or not?
            //use slog::warn;
            //warn!(ctx.logger, "No path for module `{}´", module.name());
            return Ok(());
        }
    };

    if filter && !filter_module_name_path_symbols(ctx, module, &path) {
        debug!(
            ctx.logger,
            "No name or symbol or path match for '{:?}'", path
        );
        return Ok(());
    }

    if visited.write().unwrap().insert(path.into()) {
        for m in module.dependencies() {
            install_module(ctx, kmod_ctx, &m, visited, false)?;
        }

        if let Ok((pre, _post)) = module.soft_dependencies() {
            for pre_mod in pre {
                let name =
                    pre_mod
                        .name()
                        .ok_or_else(|| "pre_mod_error")
                        .context(InstallModuleError(format!(
                            "Failed to get name for {:?}",
                            pre_mod
                        )))?;
                let it = kmod_ctx
                    .module_new_from_lookup(&OsString::from(&name))
                    .context(InstallModuleError(format!("Failed lookup for {:?}", name)))?;
                for m in it {
                    debug!(ctx.logger, "pre <{:?}>", m.path());
                    install_module(ctx, kmod_ctx, &m, visited, false)?;
                }
            }
        }
    } else {
        debug!(ctx.logger, "cache hit <{:?}>", module.name());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    use slog::*;
    use slog_async::OverflowStrategy;
    use slog_term;
    use tempfile::TempDir;

    #[test]
    fn test_modules() {
        let tmpdir = TempDir::new_in("/var/tmp").unwrap().into_path();
        let mut ctx: RunContext = RunContext {
            destrootdir: tmpdir,
            module: true,
            loglevel: Level::Info,
            ..Default::default()
        };

        let decorator = slog_term::TermDecorator::new().build();
        let drain = slog_term::FullFormat::new(decorator)
            .use_original_order()
            .build()
            .filter_level(ctx.loglevel)
            .fuse();
        let drain = slog_async::Async::new(drain)
            .overflow_strategy(OverflowStrategy::Block)
            .build()
            .fuse();
        ctx.logger = Logger::root(drain, o!());

        if let Err(e) = install_modules(&mut ctx, &vec![OsString::from("=")]) {
            panic!("Error: {:?}", e);
        }
    }
    #[test]
    fn test_usr() {
        use std::fs::read_dir;

        let tmpdir = TempDir::new_in("/var/tmp").unwrap().into_path();

        let files = read_dir("/usr/bin")
            .unwrap()
            .map(|e| OsString::from(e.unwrap().path().as_os_str()))
            .collect::<Vec<_>>();
        let mut res = ldd(&files, false, &tmpdir);
        eprintln!("no. files = {}", res.len());
        let hs: HashSet<OsString> = res.iter().cloned().collect();
        eprintln!("no. unique files = {}", hs.len());
        res.sort();
        eprintln!("files = {:#?}", res);
    }

    #[test]
    fn test_libe() {
        let tmpdir = TempDir::new_in("/var/tmp").unwrap().into_path();
        /*
                let files = read_dir("/usr/bin")
                    .unwrap()
                    .map(|e| OsString::from(e.unwrap().path().as_os_str()))
                    .collect::<Vec<_>>();
        */
        let mut files = Vec::<OsString>::new();
        files.push(OsString::from("/usr/lib64/epiphany/libephymain.so"));

        let mut res = ldd(&files, false, &tmpdir);
        eprintln!("no. files = {}", res.len());
        let hs: HashSet<OsString> = res.iter().cloned().collect();
        eprintln!("no. unique files = {}", hs.len());
        res.sort();
        eprintln!("files = {:#?}", res);
    }
}

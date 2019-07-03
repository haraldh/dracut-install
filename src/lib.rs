#![allow(dead_code)]

use kmod2 as kmod;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::io::{BufReader, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::RwLock;

use hashbrown::HashSet;
use rayon::prelude::*;

use slog::{debug, o, Level, Logger};
use walkdir::WalkDir;

use chainerror::*;

use regex::bytes::Regex;

use crate::elfkit::ld_so_cache::LDSOCache;
use crate::elfkit::ldd::Ldd;
use crate::file::clone_path;

mod acl;
mod cstrviter;
mod elfkit;
mod file;
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
    let mut str_table = Vec::<u8>::new();
    let sysroot = OsStr::new("/");
    let cache = LDSOCache::read_ld_so_cache(sysroot, &mut str_table).ok();

    let standard_libdirs = vec![OsString::from("/lib64/dyninst"), OsString::from("/lib64")];
    let visited = RwLock::new(HashSet::<OsString>::new());
    let ldd = Ldd::new(cache.as_ref(), &standard_libdirs, dest_path);
    let mut _buf = Vec::<u8>::new();

    let lpaths = HashSet::new();

    let dest = OsString::from(dest_path.as_os_str());

    files
        .par_iter()
        .flat_map(|path| {
            let path: OsString = PathBuf::from(path)
                .canonicalize()
                .unwrap()
                .as_os_str()
                .into();

            if visited.write().unwrap().insert(path.clone()) {
                let mut dest = dest.clone();
                dest.push(path.as_os_str());
                let dest = PathBuf::from(dest);
                if !dest.exists() {
                    let mut deps = ldd.recurse(&path, &lpaths, &visited).unwrap_or_else(|e| {
                        if report_error {
                            let stderr = io::stderr();
                            let mut stderr = stderr.lock();
                            let _ = stderr.write_all(path.as_bytes());
                            let _ = stderr.write_all(b": ");
                            let _ = stderr.write_all(e.to_string().as_bytes());
                            let _ = stderr.write_all(b"\n");
                        }
                        vec![]
                    });
                    deps.push(path);
                    deps
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        })
        .collect::<Vec<_>>()
}

pub fn modalias_list() -> Result<HashSet<String>, Box<dyn std::error::Error>> {
    let mut modules: HashSet<String> = HashSet::new();

    let kmod_ctx = kmod::Context::new()?;

    for m in kmod_ctx.modules_loaded()? {
        modules.insert(m.name());

        for k in kmod_ctx.module_new_from_lookup(&OsString::from(m.name()))? {
            modules.insert(k.name());
        }
    }

    for entry in WalkDir::new("/sys/devices")
        .into_iter()
        .filter_map(std::result::Result::<_, _>::ok)
        .filter(|e| e.file_name().as_bytes().eq(b"modalias"))
    {
        let f = File::open(entry.path())?;
        let mut br = BufReader::new(f);
        let mut modalias = Vec::new();
        br.read_to_end(&mut modalias)?;
        if let Some(b'\n') = modalias.last() {
            modalias.pop();
        }
        if modalias.is_empty() {
            continue;
        }
        let modalias = OsStr::from_bytes(&modalias);

        for m in kmod_ctx.module_new_from_lookup(modalias)? {
            modules.insert(m.name());
        }
    }
    Ok(modules)
}

pub fn install_files_ldd(
    ctx: &mut RunContext,
    files: &[OsString],
) -> Result<(), Box<dyn std::error::Error>> {
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
) -> Result<(), Box<dyn std::error::Error>> {
    for i in files {
        clone_path(&PathBuf::from(i), &ctx.destrootdir)?;
    }

    Ok(())
}

pub fn install_modules(
    ctx: &mut RunContext,
    module_args: &[OsString],
) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::ffi::OsStringExt;

    let visited = RwLock::new(HashSet::<OsString>::new());

    let kmod_ctx = kmod::Context::new_with(ctx.kerneldir.as_ref().map(OsString::as_os_str), None)?;

    let (module_iterators, errors): (Vec<_>, Vec<_>) = module_args
        .iter()
        .flat_map(|module| {
            if module.as_bytes().starts_with(b"/") {
                debug!(ctx.logger, "by file path");
                let m = module.to_str().unwrap();
                let name = kmod_ctx.module_new_from_path(m).unwrap().name();
                return vec![kmod_ctx.module_new_from_lookup(&OsString::from(name))];
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
                                let name = m.name();
                                kmod_ctx.module_new_from_lookup(&OsString::from(name))
                            })
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
        .map(|m| install_module_check(ctx, &kmod_ctx, &m, &visited))
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

derive_str_cherr!(InstallModuleError);

fn check_module_name_path_symbols(ctx: &RunContext, module: &kmod::Module, path: &str) -> bool {
    let name = module.name();

    if let Some(ref r) = ctx.mod_filter_noname {
        if r.is_match(name.as_bytes()) {
            return false;
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
                "Error getting dependency symbols for {}: {}",
                name,
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

fn install_module_check(
    ctx: &RunContext,
    kmod_ctx: &kmod::Context,
    module: &kmod::Module,
    visited: &RwLock<HashSet<OsString>>,
) -> ChainResult<(), InstallModuleError> {
    debug!(ctx.logger, "handling module <{}>", module.name());

    let path = match module.path() {
        Some(p) => p,
        None => {
            // FIXME: Error or not?
            //use slog::warn;
            //warn!(ctx.logger, "No path for module `{}´", module.name());
            return Ok(());
        }
    };

    if !check_module_name_path_symbols(ctx, module, &path) {
        debug!(ctx.logger, "No name or symbol or path match for '{}'", path);
        return Ok(());
    }

    if visited.write().unwrap().insert(path.into()) {
        for m in module.dependencies() {
            install_module_nocheck(ctx, kmod_ctx, &m, visited)?;
        }

        if let Ok((pre, _post)) = module.soft_dependencies() {
            for pre_mod in pre {
                let name = pre_mod.name();
                let it = kmod_ctx
                    .module_new_from_lookup(&OsString::from(&name))
                    .map_err(mstrerr!(InstallModuleError, "Failed lookup for {}", name))?;
                for m in it {
                    debug!(ctx.logger, "pre <{}>", m.name());
                    install_module_nocheck(ctx, kmod_ctx, &m, visited)?;
                }
            }
        }
    } else {
        debug!(ctx.logger, "cache hit <{}>", module.name());
    }
    Ok(())
}

fn install_module_nocheck(
    ctx: &RunContext,
    kmod_ctx: &kmod::Context,
    module: &kmod::Module,
    visited: &RwLock<HashSet<OsString>>,
) -> ChainResult<(), InstallModuleError> {
    debug!(ctx.logger, "handling module <{}>", module.name());

    let path = match module.path() {
        Some(p) => p,
        None => {
            // FIXME: Error or not?
            //use slog::warn;
            //warn!(ctx.logger, "No path for module `{}´", module.name());
            return Ok(());
        }
    };

    if visited.write().unwrap().insert(path.into()) {
        for m in module.dependencies() {
            install_module_check(ctx, kmod_ctx, &m, visited)?;
        }

        if let Ok((pre, _post)) = module.soft_dependencies() {
            for pre_mod in pre {
                let name = pre_mod.name();
                let it = kmod_ctx
                    .module_new_from_lookup(&OsString::from(&name))
                    .map_err(mstrerr!(InstallModuleError, "Failed lookup for {}", name))?;
                for m in it {
                    debug!(ctx.logger, "pre <{}>", m.name());
                    install_module_check(ctx, kmod_ctx, &m, visited)?;
                }
            }
        }
    } else {
        debug!(ctx.logger, "cache hit <{}>", module.name());
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
}

use std::borrow::Borrow;
use std::env;
use std::ffi::{CStr, OsStr, OsString};
use std::fs::OpenOptions;
use std::os::unix::prelude::*;
use std::path::PathBuf;

use bstr::BString;
use clap::{App, Arg};
use regex::bytes::Regex;
use slog::*;
use slog_async::OverflowStrategy;
use slog_term;

use dracut_install::{install_files_ldd, install_modules, modalias_list, RunContext};

//use itertools::Itertools;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const NAME: &str = env!("CARGO_PKG_NAME");

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() {
    let after_help = format!(
        r#"Example usage:

  {NAME} -D DESTROOTDIR [OPTION]... -a SOURCE...
  or: {NAME} -D DESTROOTDIR [OPTION]... SOURCE DEST
  or: {NAME} -D DESTROOTDIR [OPTION]... -m KERNELMODULE [KERNELMODULE …]

  Install SOURCE to DEST in DESTROOTDIR with all needed dependencies.

  KERNELMODULE can have one of the formats:
     * <absolute path> with a leading /
     * =<kernel subdir>[/<kernel subdir>…] like '=drivers/hid'
     * <module name>"#,
        NAME = NAME
    );

    let app = App::new(NAME)
        .version(VERSION)
        .after_help(after_help.as_ref())
        .arg(
            Arg::with_name("debug")
                .long("debug")
                .help("Show debug output"),
        )
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .long("verbose")
                .help("Show more output")
                .multiple(true),
        )
        .arg(
            Arg::with_name("version")
                .long("version")
                .help("Show package version")
                .takes_value(false)
                .required(false),
        )
        .arg(
            Arg::with_name("dir")
                .short("d")
                .long("dir")
                .help("SOURCE is a directory")
                .takes_value(false)
                .required(false),
        )
        .arg(
            Arg::with_name("ldd")
                .short("l")
                .long("ldd")
                .help("Also install shebang executables and libraries")
                .takes_value(false)
                .required(false),
        )
        .arg(
            Arg::with_name("resolvelazy")
                .short("R")
                .long("resolvelazy")
                .help("Only install shebang executables and libraries for all SOURCE files")
                .takes_value(false)
                .required(false),
        )
        .arg(
            Arg::with_name("optional")
                .short("o")
                .long("optional")
                .help("If SOURCE does not exist, do not fail")
                .takes_value(false)
                .required(false),
        )
        .arg(
            Arg::with_name("all")
                .short("a")
                .long("all")
                .help("Install all SOURCE arguments to <DESTROOTDIR>")
                .takes_value(false)
                .required(false),
        )
        .arg(
            Arg::with_name("hostonly")
                .short("H")
                .long("hostonly")
                .help("Mark all SOURCE files as hostonly")
                .takes_value(false)
                .required(false),
        )
        .arg(
            Arg::with_name("module")
                .short("m")
                .long("module")
                .help("Install kernel modules, instead of files")
                .takes_value(false)
                .required(false),
        )
        .arg(
            Arg::with_name("fips")
                .short("f")
                .long("fips")
                .help("Also install all '.SOURCE.hmac' files")
                .takes_value(false)
                .required(false),
        )
        .arg(
            Arg::with_name("modalias")
                .long("modalias")
                .help("Only generate module list from /sys/devices modalias list")
                .takes_value(false)
                .required(false),
        )
        .arg(
            Arg::with_name("silent")
                .long("silent")
                .help("Don't display error messages for kernel module install")
                .takes_value(false)
                .required(false),
        )
        .arg(
            Arg::with_name("destrootdir")
                .short("D")
                .long("destrootdir")
                .value_name("DESTROOTDIR")
                .help("Install all files to <DESTROOTDIR> as the root")
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("logdir")
                .short("L")
                .long("logdir")
                .value_name("DIR")
                .help("Log files, which were installed from the host to <DIR>")
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("mod-filter-path")
                .short("p")
                .long("mod-filter-path")
                .value_name("REGEXP")
                .help("Filter kernel modules by path <REGEXP>")
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("mod-filter-nopath")
                .short("P")
                .long("mod-filter-nopath")
                .value_name("REGEXP")
                .help("Exclude kernel modules by path <REGEXP>")
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("mod-filter-symbol")
                .short("s")
                .long("mod-filter-symbol")
                .value_name("REGEXP")
                .help("Filter kernel modules by symbol <REGEXP>")
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("mod-filter-nosymbol")
                .short("S")
                .long("mod-filter-nosymbol")
                .value_name("REGEXP")
                .help("Exclude kernel modules by symbol <REGEXP>")
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("mod-filter-noname")
                .short("N")
                .long("mod-filter-noname")
                .value_name("REGEXP")
                .help("Exclude kernel modules by name <REGEXP>")
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("kerneldir")
                .long("kerneldir")
                .value_name("DIR")
                .help("Specify the kernel module directory")
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("firmwaredirs")
                .long("firmwaredirs")
                .value_name("DIRS")
                .help("Specify the firmware directory search path with : separation")
                .takes_value(true)
                .required(false),
        )
        .arg(Arg::from_usage("<arg>... 'files, directories or kernel modules'").required(false));

    let matches = app.get_matches();

    let mut ctx: RunContext = RunContext {
        destrootdir: if let Some(dir) = matches.value_of_os("destrootdir") {
            PathBuf::from(dir)
        } else {
            let /* mut */ dest_root_dir = match env::var_os("DESTROOTDIR") {
                None => {
                    if matches.is_present("modalias") {
                        OsString::from("")
                    } else {
                        eprintln!("DESTROOTDIR is unset and no --destrootdir given");
                        std::process::exit(1);
                    }
                }
                Some(d) => d
            };

            PathBuf::from(dest_root_dir)
        },
        all: matches.is_present("all"),
        hmac: matches.is_present("hmac"),
        createdir: matches.is_present("createdir"),
        optional: matches.is_present("optional"),
        silent: matches.is_present("silent"),
        module: matches.is_present("module"),
        modalias: matches.is_present("modalias"),
        resolvelazy: matches.is_present("resolvelazy"),
        resolvedeps: matches.is_present("resolvedeps"),
        hostonly: matches.is_present("hostonly"),
        loglevel: if matches.is_present("debug") {
            Level::Debug
        } else {
            match matches.occurrences_of("verbose") {
                0 => Level::Warning,
                1 => Level::Info,
                2 => Level::Debug,
                3 | _ => Level::Trace,
            }
        },
        kerneldir: matches.value_of_os("kerneldir").map(OsString::from),
        logdir: matches.value_of_os("logdir").map(OsString::from),
        mod_filter_path: matches.value_of_os("mod-filter-path").map(|s| {
            let s = s.to_string_lossy();
            Regex::new(s.borrow()).unwrap_or_else(|e| {
                eprintln!("filter path '{:?}' a regexp: {}", s, e);
                std::process::exit(1)
            })
        }),
        mod_filter_nopath: matches.value_of_os("mod-filter-nopath").map(|s| {
            let s = s.to_string_lossy();
            Regex::new(s.borrow()).unwrap_or_else(|e| {
                eprintln!("filter nopath '{:?}' not a regexp: {}", s, e);
                std::process::exit(1)
            })
        }),
        mod_filter_symbol: matches.value_of_os("mod-filter-symbol").map(|s| {
            let s = s.to_string_lossy();
            Regex::new(s.borrow()).unwrap_or_else(|e| {
                eprintln!("filter symbol '{:?}' not a regexp: {}", s, e);
                std::process::exit(1)
            })
        }),
        mod_filter_nosymbol: matches.value_of_os("mod-filter-nosymbol").map(|s| {
            let s = s.to_string_lossy();
            Regex::new(s.borrow()).unwrap_or_else(|e| {
                eprintln!("filter nosymbol '{:?}' not a regexp: {}", s, e);
                std::process::exit(1)
            })
        }),
        mod_filter_noname: matches.value_of_os("mod-filter-noname").map(|s| {
            let s = s.to_string_lossy();
            Regex::new(s.borrow()).unwrap_or_else(|e| {
                eprintln!("filter noname '{:?}' not a regexp: {}", s, e);
                std::process::exit(1)
            })
        }),
        firmwaredirs: matches
            .value_of_os("firmwaredirs")
            .map(OsStr::as_bytes)
            .map(BString::from)
            .map(|s| {
                s.split(|b| *b == b':')
                    .map(OsStr::from_bytes)
                    .map(OsString::from)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        pathdirs: env::var_os("PATH")
            .iter()
            .map(OsString::as_os_str)
            .map(OsStr::as_bytes)
            .map(BString::from)
            .map(|s| {
                s.split(|b| *b == b':')
                    .map(OsStr::from_bytes)
                    .map(OsString::from)
                    .collect::<Vec<_>>()
            })
            .next()
            .unwrap_or_default(),
        logger: slog::Logger::root(slog::Discard, o!()),
    };

    let files = match matches.values_of_os("arg") {
        Some(v) => v.map(OsString::from).collect::<Vec<_>>(),
        None => Vec::<OsString>::new(),
    };

    if let Err(e) = do_main(&mut ctx, &files) {
        match ctx.loglevel {
            Level::Debug | Level::Trace => {
                error!(ctx.logger, "{:?}", e);
            }
            _ => {
                error!(ctx.logger, "Error: {}", e);
            }
        }
        drop(ctx);
        std::process::exit(1);
    }
}

fn do_main(ctx: &mut RunContext, args: &[OsString]) -> Result<()> {
    // Setup logging
    if let Some(dir) = &ctx.logdir {
        let logfile_path = PathBuf::from(dir);
        logfile_path.join(format!("{}.log", unsafe { libc::getpid() }));
        let logfile = OpenOptions::new().append(true).open(logfile_path)?;

        let decorator = slog_term::PlainDecorator::new(logfile);
        let drain = slog_term::CompactFormat::new(decorator)
            .build()
            .filter_level(ctx.loglevel)
            .fuse();
        let drain = slog_async::Async::new(drain)
            .overflow_strategy(OverflowStrategy::Block)
            .build()
            .fuse();
        ctx.logger = Logger::root(drain, o!());
    } else {
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
    }

    // Get running kernel version, if kerneldir is unset
    if ctx.kerneldir.is_none() {
        let mut utsname: libc::utsname = unsafe { std::mem::MaybeUninit::uninit().assume_init() };
        let ret = unsafe { libc::uname(&mut utsname) };
        if ret == -1 {
            return Err(Box::from(std::io::Error::last_os_error()));
        }
        ctx.kerneldir = Some({
            let mut s = OsString::from("/lib/modules/");
            s.push(OsStr::from_bytes(
                unsafe { CStr::from_ptr(&utsname.release as *const libc::c_char) }.to_bytes(),
            ));
            s
        });
    }

    if ctx.modalias {
        for m in modalias_list()? {
            println!("{:?}", m);
        }
        return Ok(());
    }

    if ctx.module {
        install_modules(ctx, args)
    } else {
        install_files_ldd(ctx, args)
    }
}

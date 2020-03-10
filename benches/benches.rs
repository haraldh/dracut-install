#![cfg(test)]
#![feature(test)]

extern crate test;

use std::default::Default;
use std::ffi::OsString;
use std::fs::read_dir;
use test::{black_box, Bencher};

use slog::*;
use tempfile::TempDir;

use dracut_install::{install_modules, ldd, RunContext};
use slog_async::OverflowStrategy;

#[bench]
fn bench_usr(b: &mut Bencher) {
    let tmpdir = TempDir::new_in("/var/tmp").unwrap().into_path();

    let files = read_dir("/usr/bin")
        .unwrap()
        .map(|e| OsString::from(e.unwrap().path().as_os_str()))
        .collect::<Vec<_>>();
    b.iter(|| {
        black_box(ldd(&files, false, &tmpdir));
    });
}

#[bench]
fn bench_modules(b: &mut Bencher) {
    let tmpdir = TempDir::new_in("/var/tmp").unwrap().into_path();
    let mut ctx: RunContext = RunContext {
        destrootdir: tmpdir,
        module: true,
        loglevel: Level::Warning,
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

    b.iter(|| {
        black_box(install_modules(&mut ctx, &vec![OsString::from("=drivers/block")]).unwrap());
    });
}

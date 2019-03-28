#![cfg(test)]
#![feature(test)]

extern crate test;

use std::ffi::OsString;
use std::fs::read_dir;
use test::{black_box, Bencher};

use itertools::Itertools;

use dracut_install::ldd;

#[bench]
fn bench_usr(b: &mut Bencher) {
    let files = &read_dir("/usr/bin")
        .unwrap()
        .map(|e| OsString::from(e.unwrap().path().as_os_str()))
        .collect_vec()[..];
    b.iter(|| {
        black_box(ldd(files, false));
    });
}

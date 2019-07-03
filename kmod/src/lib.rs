//! Bindings to libkmod to manage linux kernel modules.
//!
//! # Example
//! ```
//! extern crate kmod;
//!
//! fn main() {
//!     // create a new kmod context
//!     let ctx = kmod::Context::new().unwrap();
//!
//!     // get a kmod_list of all loaded modules
//!     for module in ctx.modules_loaded().unwrap() {
//!         let name = module.name();
//!         let refcount = module.refcount();
//!         let size = module.size();
//!
//!         let holders: Vec<_> = module.holders()
//!             .map(|x| x.name())
//!             .collect();
//!
//!         println!("{:<19} {:8}  {} {:?}", name, size, refcount, holders);
//!     }
//! }
//! ```
extern crate errno;
#[macro_use]
extern crate error_chain;
extern crate kmod_sys;
#[macro_use]
extern crate log;
extern crate core;
extern crate reduce;

pub use ctx::*;
pub use errors::{Error, ErrorKind, Result};
pub use modules::*;

mod errors {
    use std;

    use errno::Errno;

    error_chain! {
        errors {
            Errno(err: Errno) {
                description("got error")
                display("{}", err)
            }
        }
        foreign_links {
            NulError(std::ffi::NulError);
        }
    }
}

mod ctx;
mod modules;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsmod() {
        let ctx = Context::new().unwrap();

        for module in ctx.modules_loaded().unwrap() {
            let name = module.name();
            let refcount = module.refcount();
            let size = module.size();

            let holders: Vec<_> = module.holders().map(|x| x.name()).collect();

            println!("{:<19} {:8}  {} {:?}", name, size, refcount, holders);
        }
    }

}

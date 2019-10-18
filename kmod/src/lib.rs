//! Bindings to libkmod to manage linux kernel modules.
//!
//! # Example
//! ```
//! fn main() -> Result<(), Box<std::error::Error>>{
//!     // create a new kmod context
//!     let ctx = kmod::Context::new()?;
//!
//!     // get a kmod_list of all loaded modules
//!     for module in ctx.modules_loaded()? {
//!         let name = module.name().unwrap_or_default().to_string_lossy();
//!         let refcount = module.refcount();
//!         let size = module.size();
//!
//!         let holders: Vec<_> = module.holders()
//!             .map(|x| x.name().unwrap_or_default().to_string_lossy().into_owned())
//!             .collect();
//!
//!         println!("{:<19} {:8}  {} {:?}", name, size, refcount, holders);
//!     }
//!     Ok(())
//! }
//! ```
#![deny(
    warnings,
    absolute_paths_not_starting_with_crate,
    deprecated_in_future,
    keyword_idents,
    macro_use_extern_crate,
    trivial_numeric_casts,
    unused_extern_crates,
    unused_import_braces,
    unused_qualifications,
    unused_results,
    unused_labels,
    unused_lifetimes,
    unreachable_pub,
    future_incompatible,
    missing_doc_code_examples,
    rust_2018_idioms,
    rust_2018_compatibility
)]

pub use ctx::*;
pub use errno::Errno;
pub use errors::{Error, ErrorKind, Result};
pub use modules::*;

mod errors {
    use std;

    use errno::Errno;

    use error_chain::error_chain;

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
            let name = module.name().unwrap_or_default().to_string_lossy();
            let refcount = module.refcount();
            let size = module.size();

            let holders: Vec<_> = module
                .holders()
                .map(|x| x.name().unwrap_or_default().to_string_lossy().into_owned())
                .collect();

            println!("{:<19} {:8}  {} {:?}", name, size, refcount, holders);
        }
    }

    #[test]
    fn bad_name() {
        let ctx = Context::new().unwrap();
        let m = ctx
            .module_new_from_name("/lib/modules/5.1.12-300.fc30.x86_64/kernel/fs/cifs/cifs.ko.xz")
            .unwrap();
        println!("name: {:?}", m.name());
        println!("path: {:?}", m.path());
    }
}

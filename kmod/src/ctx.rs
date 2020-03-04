use std::ffi::{CStr, CString, OsStr, OsString};
use std::hint::unreachable_unchecked;
use std::{fmt, ptr};

use chainerror::*;
use errno;
use kmod_sys;
use log::trace;

use crate::errors::{ErrorKind, Result};
use crate::modules::{Module, ModuleIterator};

/// The kmod context
///
/// ```
/// let ctx = kmod::Context::new().unwrap();
/// ```
pub struct Context {
    ctx: *mut kmod_sys::kmod_ctx,
}

impl Drop for Context {
    fn drop(&mut self) {
        trace!("dropping kmod: {:?}", self.ctx);
        let _ = unsafe { kmod_sys::kmod_unref(self.ctx) };
    }
}

impl Context {
    /// Create a new kmod context.
    ///
    /// ```
    /// let ctx = kmod::Context::new().unwrap();
    /// ```
    #[inline]
    pub fn new() -> Result<Context> {
        let ctx = unsafe { kmod_sys::kmod_new(ptr::null(), ptr::null()) };

        if ctx.is_null() {
            Err(ErrorKind::Generic("kmod_new failed").into())
        } else {
            trace!("creating kmod: {:?}", ctx);
            Ok(Context { ctx })
        }
    }

    /// Create a new kmod context.
    ///
    /// ```
    /// let ctx = kmod::Context::new().unwrap();
    /// ```
    #[inline]
    pub fn new_with(dirname: Option<&OsStr>, config_paths: Option<Vec<&OsStr>>) -> Result<Context> {
        use std::os::unix::ffi::OsStrExt;

        let ctx = {
            if let Some(d) = dirname {
                let c = CString::new(d.as_bytes())
                    // Can't fail, because a Path cannot contain 0 bytes
                    .unwrap_or_else(|_| unsafe { unreachable_unchecked() });
                if let Some(cp) = config_paths {
                    let cvec = cp
                        .into_iter()
                        .map(|d| {
                            CString::new(d.as_bytes())
                                // Can't fail, because a Path cannot contain 0 bytes
                                .unwrap_or_else(|_| unsafe { unreachable_unchecked() })
                        })
                        .collect::<Vec<_>>();

                    let mut cvec_ptr = cvec.iter().map(|c| c.as_ptr()).collect::<Vec<_>>();

                    cvec_ptr.push(ptr::null());

                    unsafe { kmod_sys::kmod_new(c.as_ptr(), cvec_ptr.as_ptr()) }
                } else {
                    unsafe { kmod_sys::kmod_new(c.as_ptr(), ptr::null()) }
                }
            } else if let Some(cp) = config_paths {
                let cvec = cp
                    .into_iter()
                    .map(|d| {
                        CString::new(d.as_bytes())
                            // Can't fail, because a Path cannot contain 0 bytes
                            .unwrap_or_else(|_| unsafe { unreachable_unchecked() })
                    })
                    .collect::<Vec<_>>();

                let mut cvec_ptr = cvec.iter().map(|c| c.as_ptr()).collect::<Vec<_>>();

                cvec_ptr.push(ptr::null());

                unsafe { kmod_sys::kmod_new(ptr::null(), cvec_ptr.as_ptr()) }
            } else {
                unsafe { kmod_sys::kmod_new(ptr::null(), ptr::null()) }
            }
        };

        if ctx.is_null() {
            Err(ErrorKind::Generic("kmod_new failed").into())
        } else {
            trace!("creating kmod: {:?}", ctx);
            Ok(Context { ctx })
        }
    }

    /// Get an iterator of all loaded modules.
    ///
    /// ```
    /// let ctx = kmod::Context::new().unwrap();
    /// for module in ctx.modules_loaded().unwrap() {
    ///     // ...
    /// }
    /// ```
    #[inline]
    pub fn modules_loaded(&self) -> Result<ModuleIterator> {
        let mut list = ptr::null::<kmod_sys::kmod_list>() as *mut kmod_sys::kmod_list;
        let ret = unsafe { kmod_sys::kmod_module_new_from_loaded(self.ctx, &mut list) };

        if ret < 0 {
            Err(ErrorKind::Errno(errno::errno()).into())
        } else {
            trace!("kmod_module_new_from_loaded: {:?}", list);
            Ok(ModuleIterator::new(list))
        }
    }

    /// Create a module struct by looking up a name or alias.
    ///
    /// ```
    /// # fn main() { foo(); }
    /// # fn foo() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::ffi::{OsStr, OsString};
    /// let ctx = kmod::Context::new()?;
    /// let module = ctx.module_new_from_lookup(&OsString::from("vfat"))?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn module_new_from_lookup(&self, alias: &OsStr) -> Result<ModuleIterator> {
        use std::os::unix::ffi::OsStrExt;
        let mut list = ptr::null::<kmod_sys::kmod_list>() as *mut kmod_sys::kmod_list;
        let alias = CString::new(alias.as_bytes()).map_err(|e| cherr!(e, ErrorKind::NulError))?;
        let ret =
            unsafe { kmod_sys::kmod_module_new_from_lookup(self.ctx, alias.as_ptr(), &mut list) };

        if ret < 0 {
            Err(ErrorKind::Errno(errno::errno()).into())
        } else {
            trace!("kmod_module_new_from_lookup: {:?}", list);
            Ok(ModuleIterator::new(list))
        }
    }

    /// Create a module struct by path.
    ///
    /// ```
    /// # fn main() { foo(); }
    /// # fn foo() -> Result<(), Box<dyn std::error::Error>> {
    /// let ctx = kmod::Context::new()?;
    /// let module = ctx.module_new_from_path("foo.ko")?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn module_new_from_path(&self, filename: &str) -> Result<Module> {
        let mut module = ptr::null::<kmod_sys::kmod_module>() as *mut kmod_sys::kmod_module;

        let filename = CString::new(filename).map_err(|e| cherr!(e, ErrorKind::NulError))?;
        let ret = unsafe {
            kmod_sys::kmod_module_new_from_path(self.ctx, filename.as_ptr(), &mut module)
        };

        if ret < 0 {
            Err(ErrorKind::Errno(errno::errno()).into())
        } else {
            trace!("kmod_module_new_from_path: {:?}", module);
            Ok(Module::new(module))
        }
    }

    /// Create a module struct by name.
    ///
    /// ```
    /// let ctx = kmod::Context::new().unwrap();
    /// let module = ctx.module_new_from_name("tun").unwrap();
    /// ```
    pub fn module_new_from_name(&self, name: &str) -> Result<Module> {
        let mut module = ptr::null::<kmod_sys::kmod_module>() as *mut kmod_sys::kmod_module;

        let name = CString::new(name).map_err(|e| cherr!(e, ErrorKind::NulError))?;
        let ret =
            unsafe { kmod_sys::kmod_module_new_from_name(self.ctx, name.as_ptr(), &mut module) };

        if ret < 0 {
            Err(ErrorKind::Errno(errno::errno()).into())
        } else {
            trace!("kmod_module_new_from_name: {:?}", module);
            Ok(Module::new(module))
        }
    }

    /// Get the directory where kernel modules are stored
    ///
    /// ```
    /// let ctx = kmod::Context::new().unwrap();
    /// let dirname = ctx.dirname();
    /// ```
    pub fn dirname(&self) -> OsString {
        use std::os::unix::ffi::OsStringExt;

        let dirname = unsafe { kmod_sys::kmod_get_dirname(self.ctx) };
        let dirname = unsafe { CStr::from_ptr(dirname) };
        OsString::from_vec(dirname.to_bytes().to_vec())
    }
}

impl fmt::Debug for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("Context { .. }")
    }
}

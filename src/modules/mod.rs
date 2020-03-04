use std::ffi::OsStr;
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufReader, Read};
use std::os::unix::prelude::*;

use hashbrown::HashSet;

use walkdir::WalkDir;

pub fn modalias_list(
) -> Result<HashSet<OsString>, Box<dyn std::error::Error + 'static + Send + Sync>> {
    let mut modules: HashSet<OsString> = HashSet::new();

    let kmod_ctx = kmod::Context::new()?;

    for m in kmod_ctx.modules_loaded()? {
        let module_name = m
            .name()
            .ok_or(format!("Module {:?} has no name", m.path()))?;
        modules.insert(module_name.to_os_string());

        for k in kmod_ctx.module_new_from_lookup(&OsString::from(module_name))? {
            let module_name = k
                .name()
                .ok_or(format!("Module {:?} has no name", k.path().clone()))?;
            modules.insert(module_name.to_os_string());
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
            let module_name = m
                .name()
                .ok_or(format!("Module {:?} has no name", m.path()))?;
            modules.insert(module_name.to_os_string());
        }
    }
    Ok(modules)
}


use std::env;


//use tempfile::TempDir;
use dracut_install::ldd;
use itertools::Itertools;
use std::path::PathBuf;

fn main() -> Result<(), Box<std::error::Error>> {
    let /* mut */ destrootdir = env::var_os("DESTROOTDIR").expect("DESTROOTDIR is unset");
    let /* mut */ destpath = PathBuf::from(&destrootdir);

    let res = ldd(&env::args_os().collect_vec()[1..], true);
    for i in res {
        println!(
            "cp {} {}",
            i.to_string_lossy(),
            destpath.join(&i).to_string_lossy()
        );
    }

    Ok(())
}

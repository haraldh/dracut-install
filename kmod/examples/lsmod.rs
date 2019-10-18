extern crate env_logger;
extern crate kmod;

fn main() {
    env_logger::init();

    let ctx = kmod::Context::new().expect("kmod ctx failed");

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

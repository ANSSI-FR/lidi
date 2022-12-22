use std::env;
use std::fs;
use std::path::Path;

fn supports_scmp_act_kill_process() -> bool {
    pkg_config::Config::new()
        .atleast_version("2.4.0")
        .env_metadata(true)
        .probe("libseccomp")
        .is_ok()
}

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("const.rs");

    let code = if supports_scmp_act_kill_process() {
        "pub const DEFAULT_KILL: Action = Action::KillProcess;"
    } else {
        "pub const DEFAULT_KILL: Action = Action::KillThread;"
    };
    let code = format!("/// The default kill action, defaults to KillProcess on supported libseccomp versions and falls back to KillThread otherwise\n{}", code);

    fs::write(&dest_path, &code).unwrap();
    println!("cargo:rerun-if-changed=build.rs");
}

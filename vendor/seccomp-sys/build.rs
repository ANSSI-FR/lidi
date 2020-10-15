use std::env;

fn main() {
    let rustc_link_type = match env::var("LIBSECCOMP_LIB_TYPE") {
        Ok(mode) => mode, // static, framework, dylib
        Err(_) => String::from("dylib")
    };

    println!(
        "cargo:rustc-link-lib={}={}",
        rustc_link_type,
        "seccomp"
    );

    match env::var("LIBSECCOMP_LIB_PATH") {
        Ok(rustc_link_search) =>
            println!("cargo:rustc-link-search=native={}", rustc_link_search),
        Err(_) => {}
    };
}

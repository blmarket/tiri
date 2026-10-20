fn main() {
    let mut libraries = vec!["ncursesw", "utf8proc", "resolv"];
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        libraries.push("utempter");
    }
    for library in libraries {
        println!("cargo:rustc-link-lib={library}");
    }
}

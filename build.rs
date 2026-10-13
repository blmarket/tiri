fn main() {
    let mut libraries = vec!["ncursesw", "utf8proc", "m", "resolv"];
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        libraries.extend(["utempter", "systemd"]);
    }
    for library in libraries {
        println!("cargo:rustc-link-lib={library}");
    }
}

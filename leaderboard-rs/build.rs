//! Embed the member client (`predlab-py/src/predlab/__init__.py`) into the
//! binary at build time so the leaderboard can serve it as a single-file
//! download straight from the club domain, for members who don't want to
//! `git clone` + `uv sync`. We read the canonical file (rather than
//! committing a copy) so the served client can never drift from the real one.

use std::{env, fs, path::Path};

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let src = Path::new(&manifest).join("../predlab-py/src/predlab/__init__.py");
    let out = env::var("OUT_DIR").expect("OUT_DIR");
    let dest = Path::new(&out).join("predlab_client.py");

    println!("cargo:rerun-if-changed={}", src.display());
    let body = fs::read_to_string(&src).unwrap_or_else(|e| {
        panic!("build.rs: cannot read member client at {}: {e}", src.display())
    });
    fs::write(&dest, body).expect("write embedded client to OUT_DIR");
}

use serde_json::json;
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn command(directory: &Path, program: &str, args: &[&str]) -> String {
    let output = Command::new(program)
        .args(args)
        .current_dir(directory)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{program} {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .trim_end()
        .to_owned()
}

fn rust_files(path: &Path, output: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, output);
        } else if path.extension().is_some_and(|x| x == "rs") {
            output.push(path);
        }
    }
}

fn main() {
    let tool = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .canonicalize()
        .unwrap();
    let root = tool.join("../..").canonicalize().unwrap();
    let mut files = vec![
        root.join("Cargo.toml"),
        root.join("Cargo.lock"),
        root.join("crates/openwurli-dsp/Cargo.toml"),
        tool.join("Cargo.toml"),
        tool.join("build.rs"),
    ];
    if tool.join("Cargo.lock").exists() {
        files.push(tool.join("Cargo.lock"));
    }
    rust_files(&root.join("crates/openwurli-dsp/src"), &mut files);
    rust_files(&tool.join("src"), &mut files);
    files.sort();
    let mut blobs = BTreeMap::new();
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let relative = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let hash = command(&root, "git", &["hash-object", "--", &relative]);
        blobs.insert(relative, hash);
    }
    println!(
        "cargo:rerun-if-changed={}",
        root.join("crates/openwurli-dsp/src").display()
    );
    println!("cargo:rerun-if-changed={}", tool.join("src").display());
    for key in ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_TARGET"] {
        println!("cargo:rerun-if-env-changed={key}");
    }
    let features: BTreeMap<_, _> = env::vars()
        .filter(|(key, _)| key.starts_with("CARGO_FEATURE_"))
        .collect();
    let rustc = env::var("RUSTC").unwrap();
    let settings: BTreeMap<_, _> = [
        "PROFILE",
        "OPT_LEVEL",
        "DEBUG",
        "TARGET",
        "HOST",
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_CFG_TARGET_FEATURE",
    ]
    .into_iter()
    .map(|key| (key, env::var(key).ok()))
    .collect();
    let data = json!({
        "root": root, "git_head": command(&root,"git", &["rev-parse", "HEAD"]),
        "git_status": command(&root,"git", &["status", "--short"]),
        "compiler": command(&tool,&rustc,&["--version","--verbose"]),
        "settings": settings, "tool_features": features,
        "dependency_features": "openwurli-dsp defaults plus runtime-models and selected tool study features",
        "source_git_blob_hashes": blobs,
        "production_diff": command(&root,"git", &["diff", "HEAD", "--", "Cargo.toml", "crates/openwurli-dsp"])
    });
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("provenance.json"),
        serde_json::to_vec_pretty(&data).unwrap(),
    )
    .unwrap();
}

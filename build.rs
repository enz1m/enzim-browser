//! Prepare bundled rule sources and runtime-specific native filter bytecode.

#[path = "src/cosmetic_engine.rs"]
mod cosmetic_engine;
#[path = "src/filter_rules.rs"]
mod filter_rules;

use sha2::Digest;

use std::{env, fmt::Write as FmtWrite, fs, io::Write as IoWrite, path::PathBuf, process::Command};

fn main() {
    let mut cargo = std::io::stdout().lock();
    for path in [
        "src/filter_rules.rs",
        "src/cosmetic_engine.rs",
        "resources/filters",
        "tools/compile_native_filters.py",
        "DEPENDENCIES.json",
    ] {
        writeln!(cargo, "cargo:rerun-if-changed={path}").expect("publish build dependency");
    }
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("build output directory"));
    fs::write(
        out.join("cosmetic-engine.bin"),
        cosmetic_engine::build(false).serialize(),
    )
    .expect("write prepared cosmetic engine");
    let modes = [5, 6, 9, 10];
    let bundles =
        modes.map(|mode| filter_rules::convert(mode, true).expect("bundled filter conversion"));
    for (mode, bundle) in modes.iter().zip(&bundles) {
        fs::write(out.join(format!("filters-{mode}.json")), &bundle.json)
            .expect("write filter source");
    }
    let native = env::var_os("CARGO_FEATURE_WPE").is_some()
        && env::var("HOST").ok() == env::var("TARGET").ok();
    if native {
        let status = Command::new("python3")
            .arg(root.join("tools/compile_native_filters.py"))
            .arg(&out)
            .status()
            .expect("start native filter compiler");
        assert!(status.success(), "native filter preparation failed");
    }
    let mut generated = String::from(
        "type BundledFilter = (&'static str, &'static [u8], usize, usize, usize);\nfn bundled(mode: usize) -> Option<BundledFilter> {\nmatch mode {\n",
    );
    for (mode, bundle) in modes.iter().zip(&bundles) {
        let bytecode = if native {
            format!("include_bytes!(concat!(env!(\"OUT_DIR\"), \"/native/{mode}.bin\"))")
        } else {
            "&[]".to_owned()
        };
        writeln!(generated, "{mode} => Some((include_str!(concat!(env!(\"OUT_DIR\"), \"/filters-{mode}.json\")), {bytecode}, {}, {}, {})),", bundle.input, bundle.converted, bundle.rules).expect("write generated rule match");
    }
    generated.push_str("_ => None,\n}\n}\n");
    generated.push_str("fn bundled_identity(mode: usize) -> Option<&'static str> { match mode {\n");
    for (mode, bundle) in modes.iter().zip(&bundles) {
        let identifier = format!(
            "enzui-rules-v1-{:x}",
            sha2::Sha256::digest(bundle.json.as_bytes())
        );
        writeln!(generated, "{mode} => Some({identifier:?}),")
            .expect("write prepared native identity");
    }
    generated.push_str("_ => None,\n}\n}\n");
    fs::write(out.join("bundled_filters.rs"), generated).expect("write bundled filter module");
}

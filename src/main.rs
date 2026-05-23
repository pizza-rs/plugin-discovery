//! plugin-discovery
//!
//! Auto-discover Pizza analysis plugin crates and generate a meta-crate that
//! wires every plugin's `register_all(&mut AnalysisFactory)` into a single
//! `pizza_analysis_all::register_all(&mut factory)` call — the Rust equivalent
//! of Go's blank-import side-effect registration.
//!
//! # Discovery rules
//!
//! For each immediate subdirectory of every `--dir`:
//!   1. Must contain `Cargo.toml` with a `name = "<prefix>*"` line
//!      (default prefix: `pizza-analysis-`).
//!   2. Must contain `src/**/*.rs` with a line matching
//!      `pub fn register_all(`.
//!
//! Matching crates are emitted as feature-gated calls in
//! `<out>/src/lib.rs`, and listed as optional deps + features in
//! `<out>/Cargo.toml`.
//!
//! # Usage
//!
//! ```bash
//! cargo run -p pizza-plugin-discovery -- \
//!     --dir ../ \
//!     --out ../analysis-all
//! ```

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

#[derive(Debug)]
struct Plugin {
    /// Cargo package name (e.g. `pizza-analysis-stemmers`)
    crate_name: String,
    /// Rust identifier (e.g. `pizza_analysis_stemmers`)
    ident: String,
    /// Feature name in the meta-crate (e.g. `stemmers`)
    feature: String,
    /// Path to the crate root, relative to the meta-crate (e.g. `../analysis-stemmers`)
    rel_path: String,
    /// Fully-qualified `register_all` path
    register_path: String,
}

fn main() {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut out_dir: Option<PathBuf> = None;
    let mut prefix = String::from("pizza-analysis-");
    let mut crate_name = String::from("pizza-analysis-all");

    let args: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dir" => {
                i += 1;
                dirs.push(PathBuf::from(&args[i]));
            }
            "--out" => {
                i += 1;
                out_dir = Some(PathBuf::from(&args[i]));
            }
            "--prefix" => {
                i += 1;
                prefix = args[i].clone();
            }
            "--name" => {
                i += 1;
                crate_name = args[i].clone();
            }
            "-h" | "--help" => {
                print_usage();
                return;
            }
            other => {
                eprintln!("unknown flag: {other}");
                print_usage();
                process::exit(2);
            }
        }
        i += 1;
    }

    if dirs.is_empty() || out_dir.is_none() {
        print_usage();
        process::exit(2);
    }
    let out_dir = out_dir.unwrap();

    // Collect plugins from every search dir
    let mut plugins: BTreeMap<String, Plugin> = BTreeMap::new();
    for dir in &dirs {
        match fs::read_dir(dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if !p.is_dir() {
                        continue;
                    }
                    if let Some(plugin) = inspect_crate(&p, &prefix, &out_dir) {
                        plugins.insert(plugin.crate_name.clone(), plugin);
                    }
                }
            }
            Err(e) => {
                eprintln!("warning: cannot read --dir {}: {e}", dir.display());
            }
        }
    }

    if plugins.is_empty() {
        eprintln!("no plugins discovered (prefix={prefix:?})");
        process::exit(1);
    }

    println!("Discovered {} plugin(s):", plugins.len());
    for p in plugins.values() {
        println!("  - {} (feature: {})", p.crate_name, p.feature);
    }

    let plugins: Vec<Plugin> = plugins.into_values().collect();

    fs::create_dir_all(out_dir.join("src")).expect("create out src dir");
    let cargo_toml = render_cargo_toml(&crate_name, &plugins);
    let lib_rs = render_lib_rs(&plugins);

    fs::write(out_dir.join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");
    fs::write(out_dir.join("src").join("lib.rs"), lib_rs).expect("write lib.rs");

    println!("Wrote {} and {}", out_dir.join("Cargo.toml").display(), out_dir.join("src/lib.rs").display());
}

fn print_usage() {
    eprintln!(
        "plugin-discovery — auto-generate the pizza analysis meta-crate

Usage:
  plugin-discovery --dir <PATH> [--dir <PATH> ...] --out <PATH> [--prefix <STR>] [--name <STR>]

Flags:
  --dir     <PATH>  Directory to scan (repeatable). Immediate subdirs are inspected.
  --out     <PATH>  Output meta-crate root (Cargo.toml + src/lib.rs are written here).
  --prefix  <STR>   Only include crates whose Cargo package name starts with this prefix.
                    [default: pizza-analysis-]
  --name    <STR>   Cargo package name for the generated meta-crate.
                    [default: pizza-analysis-all]"
    );
}

fn inspect_crate(crate_dir: &Path, prefix: &str, out_dir: &Path) -> Option<Plugin> {
    let cargo_toml = crate_dir.join("Cargo.toml");
    if !cargo_toml.is_file() {
        return None;
    }
    let toml = fs::read_to_string(&cargo_toml).ok()?;
    let crate_name = extract_package_name(&toml)?;
    if !crate_name.starts_with(prefix) {
        return None;
    }
    // Skip the meta-crate itself and the discovery tool
    if crate_name == "pizza-analysis-all" || crate_name == "pizza-plugin-discovery" {
        return None;
    }
    if !has_register_all(&crate_dir.join("src"))? {
        return None;
    }
    let ident = crate_name.replace('-', "_");
    let feature = crate_name
        .strip_prefix(prefix)
        .unwrap_or(&crate_name)
        .to_string();
    let rel_path = relpath_from(out_dir, crate_dir);
    let register_path = format!("{ident}::register_all");
    Some(Plugin {
        crate_name,
        ident,
        feature,
        rel_path,
        register_path,
    })
}

fn extract_package_name(toml: &str) -> Option<String> {
    // Tiny parser: find the [package] section and a `name = "..."` line.
    let mut in_package = false;
    for raw in toml.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if in_package {
            if let Some(rest) = line.strip_prefix("name") {
                // name = "foo"
                let rest = rest.trim_start();
                let rest = rest.strip_prefix('=')?.trim();
                let rest = rest.strip_prefix('"')?;
                let end = rest.find('"')?;
                return Some(rest[..end].to_string());
            }
        }
    }
    None
}

fn has_register_all(src_dir: &Path) -> Option<bool> {
    if !src_dir.is_dir() {
        return Some(false);
    }
    let needle = "pub fn register_all";
    let mut stack = vec![src_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).ok()?.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|e| e.to_str()) == Some("rs") {
                if let Ok(s) = fs::read_to_string(&p) {
                    if s.contains(needle) {
                        return Some(true);
                    }
                }
            }
        }
    }
    Some(false)
}

/// Compute a relative path from `from` to `to`, using forward slashes
/// (compatible with Cargo on every platform).
fn relpath_from(from: &Path, to: &Path) -> String {
    let from = absolutize(from);
    let to = absolutize(to);
    let from_parts: Vec<_> = from.components().collect();
    let to_parts: Vec<_> = to.components().collect();
    let mut common = 0;
    while common < from_parts.len() && common < to_parts.len() && from_parts[common] == to_parts[common] {
        common += 1;
    }
    let ups = from_parts.len() - common;
    let mut out: Vec<String> = (0..ups).map(|_| "..".to_string()).collect();
    for c in &to_parts[common..] {
        out.push(c.as_os_str().to_string_lossy().into_owned());
    }
    if out.is_empty() {
        ".".to_string()
    } else {
        out.join("/")
    }
}

fn absolutize(p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        env::current_dir().unwrap_or_default().join(p)
    }
}

fn render_cargo_toml(name: &str, plugins: &[Plugin]) -> String {
    let mut s = String::new();
    s.push_str("# GENERATED BY plugin-discovery — DO NOT EDIT.\n");
    s.push_str("# Re-run: cargo run -p pizza-plugin-discovery -- --dir <contrib> --out <this-crate>\n\n");
    s.push_str("[package]\n");
    s.push_str(&format!("name = \"{name}\"\n"));
    s.push_str("version = \"0.1.0\"\n");
    s.push_str("edition = \"2021\"\n");
    s.push_str(
        "description = \"Meta-crate that wires every Pizza analysis plugin into AnalysisFactory.\"\n",
    );
    s.push_str("license = \"Apache-2.0\"\n\n");

    s.push_str("[dependencies]\n");
    s.push_str("pizza-engine = { path = \"../../lib/engine\", default-features = false }\n");
    for p in plugins {
        s.push_str(&format!(
            "{} = {{ path = \"{}\", optional = true, default-features = false }}\n",
            p.crate_name, p.rel_path
        ));
    }

    s.push_str("\n[features]\n");
    // 'default' enables every discovered plugin so `cargo build` Just Works.
    let names: Vec<String> = plugins.iter().map(|p| format!("\"{}\"", p.feature)).collect();
    s.push_str(&format!("default = [{}]\n", names.join(", ")));
    s.push_str("# Opt-out of everything via `default-features = false`, then enable selectively.\n");
    for p in plugins {
        s.push_str(&format!("{} = [\"dep:{}\"]\n", p.feature, p.crate_name));
    }

    // Treat the generated meta-crate as its own workspace root so it builds
    // standalone — contrib/ has no parent workspace.
    s.push_str("\n[workspace]\n");
    s
}

fn render_lib_rs(plugins: &[Plugin]) -> String {
    let mut s = String::new();
    s.push_str("//! GENERATED BY plugin-discovery — DO NOT EDIT.\n");
    s.push_str("//!\n");
    s.push_str("//! Meta-crate that wires every discovered Pizza analysis plugin into a single\n");
    s.push_str("//! [`AnalysisFactory`]. Each plugin is gated behind a Cargo feature so consumers\n");
    s.push_str("//! can opt out individually via `default-features = false` + selective features.\n");
    s.push_str("//!\n");
    s.push_str("//! Regenerate with:\n");
    s.push_str("//!     cargo run -p pizza-plugin-discovery -- --dir <contrib> --out <this-crate>\n\n");
    s.push_str("#![no_std]\n\n");
    s.push_str("use pizza_engine::analysis::AnalysisFactory;\n\n");
    s.push_str("/// Register every enabled plugin into `factory`.\n");
    s.push_str("///\n");
    s.push_str("/// Call order matches discovery order (alphabetical by crate name); plugins\n");
    s.push_str("/// registered later may override analyzers/filters of the same name registered\n");
    s.push_str("/// earlier (e.g. `analysis-stemmers` overrides several stop-only language\n");
    s.push_str("/// analyzers from `analysis-core`).\n");
    s.push_str("pub fn register_all(factory: &mut AnalysisFactory) {\n");
    for p in plugins {
        s.push_str(&format!("    #[cfg(feature = \"{}\")]\n", p.feature));
        s.push_str(&format!("    {}(factory);\n", p.register_path));
    }
    s.push_str("}\n\n");
    s.push_str("/// Names of all plugins compiled into this build.\n");
    s.push_str("pub fn enabled_plugins() -> &'static [&'static str] {\n");
    s.push_str("    &[\n");
    for p in plugins {
        s.push_str(&format!("        #[cfg(feature = \"{}\")]\n", p.feature));
        s.push_str(&format!("        \"{}\",\n", p.crate_name));
    }
    s.push_str("    ]\n");
    s.push_str("}\n");
    s
}

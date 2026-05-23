# plugin-discovery

Auto-discover Pizza analysis plugin crates and (re-)generate the
[`pizza-analysis-all`](https://github.com/pizza-rs/analysis-all) meta-crate that wires every plugin's
`register_all(&mut AnalysisFactory)` into a single call.

This is the Rust equivalent of the Go `cmd/plugin-discovery` tool used by the
INFINI Framework to scan for plugins and emit a blank-import file.

## How it works

Rust has no `func init()` side-effect-on-import, so we use **build-time code
generation**:

1. Scan every immediate subdirectory of each `--dir`.
2. Keep crates whose `Cargo.toml` package name starts with `--prefix`
   (default `pizza-analysis-`) **and** whose `src/` exposes a
   `pub fn register_all(...)` function.
3. Emit `<out>/Cargo.toml` listing each plugin as an optional dependency, with
   one Cargo feature per plugin (so consumers can opt out individually).
4. Emit `<out>/src/lib.rs` with a single `pub fn register_all(&mut factory)`
   that calls each enabled plugin behind `#[cfg(feature = "...")]`.

## Usage

```bash
# Build the tool
cd contrib/plugin-discovery
cargo build --release

# Regenerate the meta-crate
./target/release/plugin-discovery \
    --dir   /path/to/pizza/contrib \
    --out   /path/to/pizza/contrib/analysis-all
```

Flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--dir <PATH>` | required, repeatable | Directory to scan (immediate children only). |
| `--out <PATH>` | required | Output meta-crate root. `Cargo.toml` and `src/lib.rs` are overwritten. |
| `--prefix <STR>` | `pizza-analysis-` | Only include crates whose package name starts with this. |
| `--name <STR>` | `pizza-analysis-all` | Cargo package name for the generated meta-crate. |

## Plugin authoring convention

For a contrib crate to be auto-discovered, it must:

1. Be named `pizza-analysis-<something>` (or match a custom `--prefix`).
2. Re-export `register_all` at the crate root:

   ```rust
   // src/lib.rs
   pub use crate::register::register_all;
   ```

   where `register_all` has the signature:

   ```rust
   pub fn register_all(factory: &mut pizza_engine::analysis::AnalysisFactory) { ... }
   ```

That's it — re-run `plugin-discovery` and the new plugin is wired in.

## Application usage

After generation, downstream apps depend on the meta-crate:

```toml
[dependencies]
pizza-analysis-all = { path = "../contrib/analysis-all" }
# or, opt-in only to specific plugins:
# pizza-analysis-all = { path = "...", default-features = false, features = ["core", "stemmers"] }
```

```rust
use pizza_engine::analysis::AnalysisFactory;

let mut factory = AnalysisFactory::new();
pizza_analysis_all::register_all(&mut factory);

// Discover what was compiled in:
for name in pizza_analysis_all::enabled_plugins() {
    println!("loaded plugin: {name}");
}
```

## Comparison with the Go tool

| Aspect | Go `cmd/plugin-discovery` | Rust `plugin-discovery` |
|--------|---------------------------|-------------------------|
| Trigger | `func init()` detection | `pub fn register_all` detection |
| Output | Blank imports (`_ "path"`) | Optional deps + feature-gated `register_all` calls |
| Selectivity | All-or-nothing | Per-plugin via Cargo features |
| Re-run | Manual / build step | Manual / build step |

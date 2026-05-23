<div align="center">

# 🔍 pizza-plugin-discovery

**Auto-discovery tool for [INFINI Pizza](https://pizza.rs) analysis plugins**

[![Crate](https://img.shields.io/badge/crate-plugin--discovery-blue)](https://github.com/pizza-rs/plugin-discovery)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)

</div>

---

## Overview

`plugin-discovery` scans `contrib/*` crates and auto-generates the [`pizza-analysis-all`](https://github.com/pizza-rs/analysis-all) meta-crate that wires every plugin's `register_all(&mut AnalysisFactory)` into a single call.

### How It Works

1. Scans all directories matching `contrib/analysis-*`
2. Detects crates exporting `pub fn register_all(factory: &mut AnalysisFactory)`
3. Generates `pizza-analysis-all/src/lib.rs` with conditional compilation guards
4. Updates `pizza-analysis-all/Cargo.toml` with proper feature flags

## Usage

```bash
cargo run -p plugin-discovery
```

## License

Apache-2.0

---

<div align="center">
<sub>Part of the <a href="https://pizza.rs">INFINI Pizza</a> ecosystem</sub>
</div>

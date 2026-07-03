# AGENTS.md

## Cursor Cloud specific instructions

Vello is a Rust **Cargo workspace** (a 2D vector-graphics rendering engine). There is no server, database, or JavaScript app — the "products" are Rust libraries plus example binaries and snapshot test harnesses. Standard commands live in `README.md`, `.github/workflows/ci.yml`, and per-crate `Cargo.toml` files; prefer those. Notes below are the non-obvious caveats for this environment.

### Toolchain / environment (already provisioned in the VM snapshot)
- Uses Rust **edition 2024** with **MSRV 1.88** (CI pins stable `1.95`; the snapshot has stable `1.96`). The Ubuntu-default `rustc` (1.83) is too old — always use the rustup `stable` toolchain.
- The default `cc`/`c++` alternatives point to **clang**, but the `nv-flip-sys` C++ dependency (pulled in by `vello_tests`) only links against gcc's `libstdc++`. This VM sets `cc` and `c++` alternatives to **gcc/g++** (`update-alternatives --set cc /usr/bin/gcc`, `... c++ /usr/bin/g++`). If linking `vello_tests` fails with `unable to find library -lstdc++`, re-apply those alternatives.

### GPU / rendering (no physical GPU in the VM)
- There is no hardware GPU. GPU rendering works via the **lavapipe (llvmpipe) software Vulkan** driver. To run any GPU code (the `vello` renderer, `headless` example, `vello_tests`), export:
  - `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`
  - `WGPU_BACKEND=vulkan`
  - For GPU snapshot tests also set `VELLO_CI_GPU_SUPPORT=yes`.
- Without those, `wgpu` finds no adapter and GPU paths fail with "No compatible device found". Software rendering is correct but slow.
- The CPU-only crates (`vello_cpu`, `vello_common`, `glifo`) need no GPU and no env vars.

### Tests
- Snapshot tests compare against reference images stored in **git LFS** (`vello_tests/snapshots/*.png`, `sparse_strips/vello_sparse_tests/snapshots/*.png`). Run `git lfs pull` to materialize them. After `git lfs pull`, `git status` may list these `.png` files as "modified" — this is a git-lfs smudge quirk; **do not commit them**. If LFS is unavailable, set `VELLO_SKIP_LFS_SNAPSHOTS=all` to skip those tests.
- Tests use `cargo nextest` (installed) and are run `--release` because CPU shaders are extremely slow unoptimized (see `ci.yml`). Doc tests still use `cargo test --doc` (nextest can't run them).

### Quick reference (run from repo root)
- Build (dev): `cargo build --workspace`
- Lint: `cargo fmt --all --check` and `cargo clippy --workspace`
- CPU tests: `cargo nextest run -p vello_cpu -p vello_common -p glifo --release`
- GPU snapshot tests: with the GPU env vars above, `cargo nextest run -p vello_tests --release`
- Run CPU example → PNG: `cargo run -p vello_cpu --example basic` (writes `example_basic1.png` / `example_basic2.png` to the cwd)
- Run GPU headless render → PNG: with the GPU env vars above, `cargo run -p headless -- --test-scenes -s 0 -x 512 -y 512`
- Interactive winit demo (`cargo run -p with_winit`) needs a display; use `xvfb-run` for headless.

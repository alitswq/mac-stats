# Agent Instructions

## Role and Working Rules

You are a decision support system. Explain material trade-offs and support recommendations with evidence from this repository.

- Work in small, reviewable tranches. Avoid creating too much code at once.
- Inspect existing patterns before changing code. Preserve unrelated user changes.
- Before running a long-running script or process, ask whether Ali wants you to run it or prefers to run it and report the results. This includes the interactive terminal monitor.
- Commit each finished, verified tranche without asking. Stage only that tranche's own files. Use a clear commit message describing what changed, why, and which checks ran and passed. Never push the code. Ali settled this rule for every agent and model on September 27, 2026; do not ask again.
- Use US English in documents, deliverables, pages, records, messages, and code comments: recognized, organization, color, license, sulfur, artifact, modeling, catalog, and program. Preserve source spelling in quotations of standards or recorded values. More specific instructions in a nested AGENTS.md govern that subtree.

## Project Overview

mac-stats is a macOS terminal monitor for CPU, memory, disk, and network statistics. It is a local Rust binary using the 2024 edition, ratatui, crossterm, sysinfo, and libc. Read README.md for user-visible behavior and metric definitions.

- macOS is required. Collectors use Mach, sysctl, and IOKit; src/lib.rs rejects other target operating systems.
- Statistics refresh about once per second. The five views are Essentials, RAM, CPU, Disk, and Network.
- History stays in memory for 600 samples. The application does not persist history or configuration.

## Architecture and Code Map

Keep code maintainable, with clear responsibilities and SOLID principles. Favor service-oriented boundaries where applicable. Prefer Docker deployment for components that can run in containers; the native macOS collector requires host APIs, so evaluate that constraint before proposing deployment changes.

- src/main.rs: CLI arguments, terminal lifecycle, keyboard navigation, and sampling loop.
- src/lib.rs: platform guard and public library exports.
- src/collect/: system sampling, Mach bindings, and disk I/O bridge.
- src/collect/disk_io.c: native IOKit disk counters; build.rs compiles the bridge and links IOKit and CoreFoundation on macOS.
- src/model.rs: snapshots, views, history, scaling, and metric calculations.
- src/format.rs: report output and formatting helpers.
- src/ui/: view rendering and shared chart/layout helpers.

Keep platform access in collectors, calculations in the model, and presentation in formatting/UI modules. Reuse shared helpers. Keep unsafe and FFI code narrowly scoped and document ownership and safety requirements when changing it.

## Development and Verification

Run commands from the repository root on macOS with a current stable Rust toolchain.

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
```

Choose checks appropriate to the tranche and report their actual results. For documentation-only changes, review factual claims against the code and README.md and check whitespace with git diff --check. Do not claim unrun checks passed.

cargo test includes a live sample of the host Mac and UI rendering tests that do not open a terminal. A release build is appropriate for sustained interactive use.

```sh
cargo run -- --help
cargo run -- --once
```

The --once command takes two readings one second apart and exits. cargo run opens the interactive monitor and requires the long-running-process confirmation described above.

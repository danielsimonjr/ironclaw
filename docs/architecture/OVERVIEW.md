# IronClaw — Overview

A Rust agent runtime, forked from `nearai/ironclaw` as a **deliberate Windows refactor**. The
fork is not a lagging copy of upstream. Do not propose a sync to upstream.

## Layout

The repository is a Cargo **workspace**. Each member is its own crate with its own root.

| Area | Files | Holds |
|---|---|---|
| `src/` | 247 | The core runtime: session, agent loop, WASM host, configuration |
| `tools-src/` | 30 | One crate for each external tool (Gmail, Google Drive, Okta, Slack, Telegram) |
| `channels-src/` | 3 | One crate for each chat channel (Slack, Telegram, WhatsApp) |
| `tests/` | 6 | Integration tests |
| `examples/` | 1 | Example program |

## The numbers

288 source files, 141,031 lines, 2,739 public items, **14 crate roots**, 205 names that more
than one crate defines.

**14 crate roots is correct. That number is not an error.** A Cargo workspace holds one root for
each member crate, and a crate that is both a library and a binary holds two. `src/` supplies
`lib.rs` and `main.rs`; each tool crate and each channel crate supplies one more.

Read `metadata.language` in `dependency-graph.json` before you trust any count here. The value
must be `rust`.

## Documentation

| Document | Answers |
|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Why the system has this shape |
| [COMPONENTS.md](COMPONENTS.md) | What each module does |
| [DATAFLOW.md](DATAFLOW.md) | How a request travels |
| [API.md](API.md) | The public surface |
| [FILE_INVENTORY.md](FILE_INVENTORY.md) | Every file and its disposition |
| [TEST_COVERAGE.md](TEST_COVERAGE.md) | What the tests cover |
| [DEPENDENCY_GRAPH.md](DEPENDENCY_GRAPH.md) | Which module depends on which |
| [unused-analysis.md](unused-analysis.md) | Public items with no user inside the workspace |
| [duplicate-symbols.md](duplicate-symbols.md) | Names that more than one crate defines |

## Verification

Generated 2026-08-16 by `repo_map.py map`.
Regenerate: `python repo_map.py map <repo> --out <dir>` · Check: `python repo_map.py check <repo> --docs docs/architecture`

| Claim | Value | Source |
|---|---|---|
| totalSourceFiles | 288 | dependency-graph.json |
| totalLinesOfCode | 141031 | dependency-graph.json |
| totalExports | 2739 | dependency-graph.json |
| entryRoots | 14 | dependency-graph.json |
| orphanedFiles | 3 | dependency-graph.json |
| duplicateCount | 205 | duplicate-symbols.json |

**Claims that the gate cannot hold.** The area table above comes from the `byArea` and the
per-file `package` values in `file-inventory.json`. The gate checks the totals. The gate does
not check the split. The statement about the Windows fork comes from the repository history
and from `~/Github/AGENTS.md`, not from any metric.

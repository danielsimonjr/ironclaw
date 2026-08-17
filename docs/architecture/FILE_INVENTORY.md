# IronClaw — File Inventory

Every scanned file, its area and its disposition.

> **Derived from `file-inventory.json`.** IronClaw has no Markdown-emitting analyser of its
> own, so a person writes this file from that artifact. To refresh it, run `repo_map.py map`
> again and update both the tables and the Verification block.

## Scope — what the count includes

**288 files.** The scan reads `.rs` files only. The scan removes `target/`, which holds the
output of `cargo build` and of `cargo doc`.

**That removal is the reason this document has correct numbers.** Before `target/` was
removed, the tool read this repository as JavaScript and reported **118 source files**. All
118 were `.js` files, and 117 of them were `target/doc/**/sidebar-items.js` — files that
`rustdoc` generates. Every figure described the wrong files.

## By area

| Area | Files | Lines |
|---|---|---|
| `src/` | 247 | 125,262 |
| `tools-src/` | 30 | 9,603 |
| `tests/` | 6 | 3,566 |
| `channels-src/` | 3 | 2,372 |
| `examples/` | 1 | 122 |
| `build.rs` | 1 | 106 |
| **Total** | **288** | **141,031** |

## By disposition

| Disposition | Files |
|---|---|
| `reachable` | 264 |
| `build-entry` | 14 |
| `test` | 6 |
| `orphan` | **3** |
| `example` | 1 |

**`build-entry` is 14 because this workspace has 14 crate roots.** Each member crate supplies
one root, and a crate that is both a library and a binary supplies two. See OVERVIEW.md.

## The largest files

| Lines | File |
|---|---|
| 3,253 | `src/db/libsql_backend.rs` |
| 2,711 | `src/agent/agent_loop.rs` |
| 2,624 | `src/channels/wasm/wrapper.rs` |
| 2,410 | `src/channels/web/server.rs` |
| 2,271 | `src/history/store.rs` |
| 1,878 | `src/config.rs` |
| 1,759 | `tests/user_journey_integration.rs` |
| 1,650 | `src/tools/builtin/session_tools.rs` |

## The three orphans

| File | Why the tool reports it |
|---|---|
| `build.rs` | Cargo runs this file before a build. No `mod` declares it, so the module graph cannot reach it. **This is correct behaviour, not dead code.** |
| `tools-src/google-drive/src/api.rs` | No `mod` declaration reaches this file. Check whether the crate root declares it. |
| `tools-src/google-drive/src/types.rs` | The same condition as `api.rs` above. |

**`build.rs` will always show as an orphan.** Cargo invokes it by name. Read a report of three
orphans as "one expected, two to check", and not as three defects.

## Verification

Generated 2026-08-16 by `repo_map.py map`.
Regenerate: `python repo_map.py map <repo> --out <dir>` · Check: `python repo_map.py check <repo> --docs docs/architecture`

| Claim | Value | Source |
|---|---|---|
| totalFiles | 288 | file-inventory.json |
| totalSourceFiles | 288 | dependency-graph.json |
| totalLinesOfCode | 141031 | dependency-graph.json |
| orphanedFiles | 3 | dependency-graph.json |
| entryRoots | 14 | dependency-graph.json |

**Claims that the gate cannot hold.** The area table and the largest-file table add the
per-file `loc` values in `file-inventory.json`. The gate checks the totals. The gate does not
check the split. The reason for each orphan comes from a reading of the source, not from a
metric.

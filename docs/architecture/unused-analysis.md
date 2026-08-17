# IronClaw — Unused Analysis

Public items that no file inside this workspace uses.

> **Derived from `unused-analysis.json`.** A person writes this file from that artifact.

## Read this section before you read any number below

**2,635 of the 2,739 public items — 96 percent — have no user inside this workspace. Almost
all of them are live code.**

`unused-analysis` reads this repository and nothing else. A Rust library crate publishes its
`pub` surface for **downstream consumers**, and those consumers are not in the graph. For a
workspace of 14 crates that publish tools and channels, "no user inside the workspace" is
the expected state of a public item. That state is not evidence of dead code.

**Treat this list as candidates to compare against each crate's intended surface. Never treat
it as a deletion list.** The same analysis once marked 15 of 20 public items in a different
repository, and proposed 9 of them for deletion. All 9 were live code.

## What the numbers mean

| Figure | Value | Meaning |
|---|---|---|
| `unusedExportCount` | 2,635 | Public items with no user inside the workspace |
| `referencedInModuleCount` | 1,970 | Of those, items used inside their own crate |
| `unreferencedAnywhereCount` | 665 | Items with no user found anywhere in the scan |
| `noImporterFileCount` | 3 | Files that no `mod` declaration reaches |

**The 665 figure is the one worth reading.** Such an item has no user in its own crate and no
user anywhere else in the scan. Such an item is therefore either public API for a downstream
consumer, or a candidate to remove. Only a reading of the crate's intended surface separates the two.

## The three files with no importer

| File | Assessment |
|---|---|
| `build.rs` | **Expected.** Cargo runs this file by name before a build. No `mod` declares it, and none should. |
| `tools-src/google-drive/src/api.rs` | Check whether `tools-src/google-drive/src/lib.rs` declares `mod api;`. |
| `tools-src/google-drive/src/types.rs` | Check whether that same root declares `mod types;`. |

The two Google Drive files are the finding here. Only one crate shows this condition. That
points at that crate's root, and not at a pattern across the workspace.

## Verification

Generated 2026-08-16 by `repo_map.py map`.
Regenerate: `python repo_map.py map <repo> --out <dir>` · Check: `python repo_map.py check <repo> --docs docs/architecture`

| Claim | Value | Source |
|---|---|---|
| totalExports | 2739 | dependency-graph.json |
| unusedExportsCount | 2635 | dependency-graph.json |
| noImporterFileCount | 3 | unused-analysis.json |
| orphanedFiles | 3 | dependency-graph.json |

**Claims that the gate cannot hold.** The `referencedInModuleCount` and
`unreferencedAnywhereCount` splits come from the `summary` block of `unused-analysis.json`.
The assessment of each of the three files comes from a reading of the source and of Cargo's
own rules, not from a metric. The artifact carries its own caveats; read them.

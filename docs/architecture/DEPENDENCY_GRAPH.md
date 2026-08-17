# IronClaw — Dependency Graph

Which file depends on which, across the workspace.

> **Derived from `dependency-graph.json`.** A person writes this file from that artifact.
> Refresh it with `repo_map.py map` and update the Verification block.

## How an edge is found in Rust

Rust splits file inclusion from name import, and the two are not the same thing.

| Statement | What it does | Is it a file edge? |
|---|---|---|
| `mod foo;` | Adds `foo.rs` or `foo/mod.rs` to the crate | **Yes** |
| `use a::b::C;` | Brings a name into scope in a tree that `mod` already joined | No |

**The tool follows `mod` for reachability.** A module that a parent declares but never uses is
still part of the crate. If the tool followed `use` instead, every such module would report as
dead code, and the report would invent defects that do not exist.

The tool records `use` edges as well, because a reader who asks "what does this file depend
on" means the `use` list.

## The 14 crate roots

| Root | Crate |
|---|---|
| `src/lib.rs` | the core library |
| `src/main.rs` | the binary |
| `channels-src/slack/src/lib.rs` | Slack channel |
| `channels-src/telegram/src/lib.rs` | Telegram channel |
| `channels-src/whatsapp/src/lib.rs` | WhatsApp channel |
| `tools-src/gmail/src/lib.rs` | Gmail tool |
| `tools-src/google-calendar/src/lib.rs` | Google Calendar tool |
| `tools-src/google-docs/src/lib.rs` | Google Docs tool |
| `tools-src/google-drive/src/lib.rs` | Google Drive tool |
| `tools-src/google-sheets/src/lib.rs` | Google Sheets tool |
| `tools-src/google-slides/src/lib.rs` | Google Slides tool |
| `tools-src/okta/src/lib.rs` | Okta tool |
| `tools-src/slack/src/lib.rs` | Slack tool |
| `tools-src/telegram/src/lib.rs` | Telegram tool |

**Every root counts.** Reachability starts at all 14. If the tool used one root, it would mark
every module of the other 13 crates as dead code.

## Cycles

The tool reports **at least 1,667** simple cycles, and it stopped the search early.

**That number is a floor, not a total.** The tool says so in its own warning: the search hit a
safety cap of 2,000,001 backtracking steps. The internal graph holds a strongly-connected
component that is too dense for a complete search.

**Do not read 1,667 as a defect count.** In Rust a cycle between modules of one crate is
legal and common: `mod` declarations form a tree, and `use` statements cross it freely. The
number describes the shape of the graph, and not a fault.

## Cross-crate edges

The tool does **not** model a Cargo dependency between two workspace members. A `use` of
another crate resolves to no file here, so the tool classifies the target as external. To see
which crate depends on which, read each `Cargo.toml`.

## Verification

Generated 2026-08-16 by `repo_map.py map`.
Regenerate: `python repo_map.py map <repo> --out <dir>` · Check: `python repo_map.py check <repo> --docs docs/architecture`

| Claim | Value | Source |
|---|---|---|
| totalSourceFiles | 288 | dependency-graph.json |
| entryRoots | 14 | dependency-graph.json |
| reachableFiles | 278 | dependency-graph.json |
| orphanedFiles | 3 | dependency-graph.json |

**Claims that the gate cannot hold.** The cycle count is a floor, so this document states it
as one and the gate does not check it. The table of roots comes from
`reachability.roots`. The statement about Cargo dependencies comes from the resolver's own
documented limit in `resolvers/rust.py`.

**`typeOnlyCircularDeps` is deliberately absent from the table above.** A first draft of this
document put it there with a value of 0. The gate refused it. This repository truncated its
cycle search, so the tool cannot confirm or deny any cycle count. The tool says so, rather
than report a match. A number that the gate cannot check does not belong in the block that exists
to be checked.

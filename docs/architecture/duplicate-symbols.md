# IronClaw — Duplicate Symbols

Names that more than one file defines.

> **Derived from `duplicate-symbols.json`.** A person writes this file from that artifact.

## Read this section first

**205 names appear in more than one file, out of 2,140 names in total.** In a Cargo workspace
of 14 crates, most of these are correct and expected.

Rust gives each crate its own namespace. `gmail::AuthResult` and `okta::AuthResult` are
different types with the same short name, and neither has to change. The tool groups by **name only**, and states this in the artifact. A group here therefore
means "more than one place uses this name". A group does not mean "these definitions
conflict".

## The recurring shapes

| Group | Files | What it is |
|---|---|---|
| `AuthResult`, `AuthProfile`, `CredentialMapping`, `CredentialLocation` | 2 each | Each tool crate defines its own authentication types |
| `DeleteResult`, `DocumentMetadata` | 3 each | Each Google tool crate models the same API concept |
| `BatchUpdateResult`, `FormatTextOptions` | 2 each | Google Docs and Google Sheets model the same request shapes |
| `Channel`, `ContentBlock` | 2 each | The channel crates and the core both name these |
| `ConfigError` | 2 | Per-crate error types |
| `EchoTool` | 2 | A test double, defined in more than one test scope |

## Which of these is worth attention

**A duplicate matters when the two definitions must agree and nothing makes them agree.**

- `ContentBlock` and `Channel` are the pair to check first. Both the core crate and a channel
  crate define a type that crosses the boundary between them. The two definitions must stay
  compatible. A conversion at the seam, or a shared crate, is what makes that hold.
- The per-tool `AuthResult` and `AuthProfile` types are almost certainly correct as they
  stand. Each tool authenticates against a different service, and a shared type would force
  every tool to carry every service's fields.
- `EchoTool` in two test scopes is not a defect.

## Verification

Generated 2026-08-16 by `repo_map.py map`.
Regenerate: `python repo_map.py map <repo> --out <dir>` · Check: `python repo_map.py check <repo> --docs docs/architecture`

| Claim | Value | Source |
|---|---|---|
| duplicateCount | 205 | duplicate-symbols.json |
| totalSourceFiles | 288 | dependency-graph.json |
| totalExports | 2739 | dependency-graph.json |

**Claims that the gate cannot hold.** The `totalSymbols` figure of 2,140 and the per-group
file counts come from `duplicate-symbols.json`. The judgement about which groups deserve
attention comes from a reading of the source. The artifact does **not** classify a group as a
true duplicate or a legitimate one. The artifact says so in its own note. This document makes
no stronger claim.

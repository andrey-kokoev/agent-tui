# @narada-core/agent-tui

Standalone repository for the Narada terminal UI carrier. This repository is standalone for agent-tui code, but it is not self-contained for shared Narada contracts.

The Rust crate remains `narada-agent-tui`; this repository/package is published and referenced as `@narada-core/agent-tui`.

## Shared Contract Workspace

Tests and contract-backed fixtures read shared Narada files from a sibling repository at `../narada` relative to this repository root. The expected local workspace layout is:

```text
<src-root>/
  agent-tui/
  narada/
```

The sibling `narada` repository provides the carrier protocol, provider envelope, MCP fabric, and runtime contract fixtures consumed by agent-tui. Running `cargo test` without that sibling workspace will fail at compile time for `include_str!` contract fixtures.

## Commands

```powershell
cargo test
cargo build
```

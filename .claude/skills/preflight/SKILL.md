---
name: preflight
description: Run the CI-equivalent checks before committing OpenMMO changes — cargo fmt/clippy/test for Rust, and build:wasm/vitest/svelte-check/eslint/prettier for the client — selecting only the sides the diff touches. Use right before a commit or push, or when the user says "검증", "커밋 전 체크", "run the checks", "CI 통과할까".
---

# Pre-commit validation

Mirrors [.github/workflows/ci.yml](../../../.github/workflows/ci.yml). Run this
**once, immediately before committing** — not after every edit.

## 1. Decide which side to run

```bash
git status --porcelain
```

- Any of `server/ shared/ terrain/ agent-client/ tools/terrain-gen/ Cargo.*` → Rust checks.
- Any of `client/` → client checks.
- **`shared/` touches both** — it compiles into the server and into the browser
  via WASM. Run everything.
- `data-src/` touches both (server `build.rs` and `generate:csv`).

## 2. Rust — from the repo root

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

`--locked` and `--all-targets` are not optional: dropping them is the usual
reason a local pass fails in CI.

## 3. Client — from `client/`

```bash
npm run build:wasm    # required on a fresh tree and after any shared/ change
npm test
npm run check
npm run lint
npm run format:check
```

`npm run format` fixes formatting; `npm run lint:fix` fixes autofixable lint.

## 4. Before declaring it green

- Report the actual output. A skipped step is a skipped step — say so.
- New warnings count as failures under `-D warnings`.
- New asset added? `doc/assets/` must record its source and license (AI/paid
  tools: tier + generation date). Entries that fell out of use get **[미사용]**.
- Comments: the repo keeps them scarce and short. Strip anything verbose you
  introduced.
- Performance: the target is 5,000 concurrent users. Reject new per-tick
  O(players²) loops, per-frame allocations, and locks held across IO
  ([doc/DEVELOPMENT.md](../../../doc/DEVELOPMENT.md) §8).

## 5. Commit

Imperative mood, intent over mechanics, matching the existing history. The
`commit-agent` subagent runs these checks and commits in one step.

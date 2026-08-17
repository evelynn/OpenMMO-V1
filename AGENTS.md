# Agent Guidelines

Environment setup, architecture map, and common change recipes: [doc/DEVELOPMENT.md](doc/DEVELOPMENT.md).
Run `bash tools/dev-setup.sh --check` before assuming the environment is broken.

- Avoid comments in code where possible; only write them when truly necessary, keeping them short and concise.
- If you find long or verbose comments in existing code, rewrite them to be short and concise, or remove them where possible.
- When adding a new asset, record its source in the matching `doc/assets/` file, with the license (and for AI/paid tools, the tier + generation date). Mark entries that fall out of use with **[미사용]**.
- **Build to the design documents.** Implement each item as [doc/ragnarok/13_IMPLEMENTATION_DIRECTION.md](doc/ragnarok/13_IMPLEMENTATION_DIRECTION.md) specifies it, in the order [doc/DEVELOPMENT_MASTER_PLAN.md](doc/DEVELOPMENT_MASTER_PLAN.md) gives. A spec that turns out wrong gets amended in the same PR, with the reason, before the code lands — code and spec never disagree.

## Python

- When running Python in this repository, use the project virtual environment at `.venv`.
- Prefer `.venv\Scripts\python.exe` for direct Python commands.
- Prefer `uv pip install ...` for installing Python packages into the active project environment.

## Pre-Commit Validation

- Run validation only once, immediately before making a commit, not after every task.
- For frontend changes, run `npm run check` and `npm run lint`.
- For Rust changes, run `cargo fmt` and `cargo check`.

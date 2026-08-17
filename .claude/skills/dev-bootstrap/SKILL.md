---
name: dev-bootstrap
description: Set up a fresh OpenMMO clone into a runnable dev environment, or diagnose a broken one — toolchain, binary assets, generated data, WASM, terrain bake, env files. Use when the user says "개발 환경 세팅", "환경 구축", "set up the project", "왜 안 돌아가", "월드가 검게 나온다", "WASM import 실패", or hits a missing-data/missing-asset error at startup.
---

# Dev environment bootstrap

Full reference: [doc/DEVELOPMENT.md](../../../doc/DEVELOPMENT.md) §2–§3.

## Always start with the doctor

```bash
bash tools/dev-setup.sh --check
```

It reports each prerequisite as ok/warn/fail with the exact fix command and
exits non-zero if anything is missing. Never guess at the state — run it first.

## Fixing

```bash
bash tools/dev-setup.sh                  # idempotent full bootstrap
bash tools/dev-setup.sh --install-tools  # also cargo-install wasm-pack, cargo-watch
bash tools/dev-setup.sh --skip-terrain   # when only the terrain step is unwanted
bash tools/dev-setup.sh --full-terrain   # all 32x32 regions, ~73 GB
```

The bake is the expensive step (~3–5 min, and the default dev range is ~1 GB).
Check free disk before `--full-terrain`.

## Ordering constraints

1. Assets (`tools/fetch-assets.sh`) before anything that reads GLBs — the
   monster-clip and furniture-footprint generators measure real models.
2. `npm ci` before `npm run build:wasm`.
3. `build:wasm` before `vite`, and again after every `shared/` change.
4. Terrain bake before starting the server, or its terrain API 404s.

## Diagnosing by symptom

| Symptom | Cause |
|---|---|
| World renders black, `/api/terrain/...` 404 | not baked |
| Terrain ends abruptly at an invisible wall | walked outside the baked region range |
| `Cannot find module` for `src/lib/wasm` | `npm run build:wasm` never ran |
| `data/*.json` missing | `npm run generate:csv` |
| Models 404, or GLBs are tiny LFS pointer files | `bash tools/fetch-assets.sh` |
| Login button errors | `VITE_GOOGLE_CLIENT_ID` empty in `client/.env.local` |
| Map editor writes 403 | signed-in account not in `ADMIN_EMAILS` |

## Rules

- Do not commit `client/.env.local`, `agent-client/data/config.toml`, or
  anything under `data/terrain/` (except the tracked `zones/`).
- Private secrets go in `~/.cargo/config.toml`, never in the repo's
  `.cargo/config.toml` — that file is public.
- After the bootstrap, hand off to the `dev-run` skill rather than launching
  servers ad hoc.

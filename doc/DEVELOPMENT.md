# 개발 가이드 (Development Guide)

새로 클론한 저장소를 굴러가는 개발 환경으로 만들고, 매일의 개발 루프를 돌리고,
자주 하는 변경을 실수 없이 끝내기 위한 문서. 프로젝트 소개와 배포는
[README.md](../README.md), PR 규칙은 [CONTRIBUTING.md](../CONTRIBUTING.md)를 본다.

---

## 1. 5분 요약

```bash
bash tools/dev-setup.sh          # 툴체인 점검 + 에셋 + WASM + 개발용 지형 베이크
bash tools/dev-setup.sh --check  # 아무것도 바꾸지 않고 진단만
```

성공하면 터미널 3개로 개발 루프를 돈다.

```bash
# 1) 서버 (WS 10006 / REST 10007)
cargo watch -w server -w shared -w data-src -x "run -p onlinerpg-server"

# 2) shared 크레이트 → WASM 자동 재빌드
cargo watch -w shared -s "npm run build:wasm --prefix client"

# 3) 클라이언트 (Vite 10004)
npm --prefix client run dev -- --port 10004
```

브라우저에서 `http://localhost:10004/`. 로그인은 Google OAuth라서
`client/.env.local`의 `VITE_GOOGLE_CLIENT_ID`와 서버의 `GOOGLE_CLIENT_ID`가
같은 **Web 클라이언트 ID**여야 한다.

---

## 2. 사전 요구사항

| 도구 | 버전 | 용도 | 필수 |
|------|------|------|------|
| Rust / Cargo | stable | 서버·shared·terrain·agent-client | 필수 |
| `wasm32-unknown-unknown` 타깃 | — | shared → 브라우저 WASM | 필수 |
| `wasm-pack` | 최신 | WASM 번들 생성 | 필수 |
| Node.js / npm | 22+ (CI 기준) | 클라이언트, 데이터 생성 스크립트 | 필수 |
| `cargo-watch` | 최신 | 코드 변경 시 자동 재시작 | 권장 |
| Python `.venv` | 3.11+ | Blender/에셋 스크립트 (`tools/*.py`) | 에셋 작업 시 |
| Blender | 4.x | GLB 가공 스킬 | 에셋 작업 시 |

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack cargo-watch
```

디스크: 에셋 수 GB + 지형. 지형은 **개발용 부분 베이크 약 1 GB**, 전체 월드는 약 **73 GB**.

---

## 3. 부트스트랩 (수동 절차)

`tools/dev-setup.sh`가 하는 일을 손으로 하면 다음과 같다. 순서에 의미가 있다.

1. **바이너리 에셋** — 3D 모델·음악·사운드는 git이 아니라 Hugging Face에 있다.
   ```bash
   bash tools/fetch-assets.sh       # assets.lock이 바뀔 때마다 다시 실행
   ```
2. **환경 파일**
   ```bash
   cp client/.env.example client/.env.local            # VITE_GOOGLE_CLIENT_ID 채우기
   cp agent-client/data/config.toml.example agent-client/data/config.toml
   ```
   서버 쪽 공개 값(`GOOGLE_CLIENT_ID`)은 `.cargo/config.toml`의 `[env]`에 이미 있다.
   `ADMIN_EMAILS` 같은 비공개 값은 저장소가 아니라 `~/.cargo/config.toml`에 둔다.
3. **의존성 + 생성 데이터 + WASM**
   ```bash
   npm --prefix client ci
   npm --prefix client run build:wasm
   ```
   `build:wasm`은 CSV→JSON 변환, 애니메이션/가구/몬스터 클립 측정, `wasm-pack build`까지
   한 번에 돈다. **fresh clone에서는 반드시 필요**하고, `shared/`를 고칠 때마다 다시 필요하다.
4. **지형 베이크** — 하이트맵·스플랫맵·미니맵·수면 필드는 git에 없다.
   ```bash
   # 개발용: 스폰 주변 3x3 리전(약 600 MB). 시간은 전체 베이크와 비슷한 10분 안팎
   # (침식·도로 시뮬레이션이 리전 범위와 무관하게 월드 전체를 계산한다)
   cargo run -p terrain-gen --release -- bake --seed 42 \
     --region-x-min -3 --region-x-max -1 --region-z-min 3 --region-z-max 5

   # 전체 월드 (약 73 GB)
   cargo run -p terrain-gen --release -- bake --seed 42
   ```

   **원점(0,0)이 아니라 스폰 주변을 굽는다.** 스폰은
   `data-src/world.json`의 `(-1475.2, 0.7, 4741.6)`이고 그 리전은 **(-2, +4)**다 —
   원점 중심으로 구우면 1 GB를 굽고도 캐릭터가 뜨는 자리는 검게 나온다.
   `tools/dev-setup.sh`는 이 범위를 `world.json`에서 계산하므로 스폰이 옮겨져도
   따라간다. 리전 하나는 16타일 × 64유닛 = 1,024유닛이다.
   `data/terrain/worldgen.json`이 **마지막에** 쓰이므로, 이 파일이 있으면 베이크가
   끝까지 돈 것이다. 베이크하지 않으면 지형 API가 404를 내고 월드가 검게 렌더링된다.
   `data/terrain/zones/`(마을 no-spawn — `monsterSpawns` 사각형은 에디터 전용 레거시)는
   git에 있고 베이크가 건드리지 않는다.

베이크 범위 밖으로 걸어 나가면 지형이 없다. 개발 중에는 **스폰 주변**에서 논다.

---

## 4. 아키텍처 지도

```
shared/      Rust 크레이트 — 서버·클라이언트(WASM)·에이전트가 공유하는 진실
             messages.rs(WS 프로토콜), character/inventory/housing/dungeon,
             monster_ai, pathfinding, worldgen, wasm_api.rs
server/      Rust — 권위 서버. tokio + tokio-tungstenite(WS) + axum(REST)
             game_state/*: 전투·인벤·파티·거래·던전·허기·낚시 등 상태 전이
             game/*: 전투 판정, 능력치, HP
             terrain|housing|npc_schedule/routes.rs: REST 엔드포인트
terrain/     Rust — 타일 IO, 하이트 샘플링, 타일 캐시, 수면/나무 데이터
tools/terrain-gen/  오프라인 월드 베이커 (bake / preview / inspect / probe)
agent-client/ Rust — LLM로 움직이는 NPC 클라이언트. 사람과 **같은** WS 프로토콜만 사용
client/      Svelte 5 + TypeScript + Three.js(Threlte) + Vite
             lib/components: UI + 씬, lib/managers: 런타임 시스템,
             lib/stores: 상태, lib/network: WS, lib/terrain·utils: 지오메트리
data-src/    사람이 편집하는 원본 데이터 (CSV/JSON)
data/        빌드가 생성하는 JSON + 베이크된 지형/하우징 (지형은 git 제외)
doc/         설계 문서. 시스템을 건드리기 전에 해당 문서를 먼저 읽는다
```

**핵심 원칙 — 에이전트-인간 동등성**: 에이전트 전용 API를 만들지 않는다. 새 기능은
`ClientMessage`/`ServerMessage`에 얹어서 사람도 봇도 같은 경로로 쓰게 한다.

**권위 분리**: 데미지·명중·인벤토리·거래는 전부 서버 판정. 몬스터 AI는
소유자 클라이언트에서 돌고(`monsterManager.ai_tick_brain`), 서버는 결과를 검증한다.

### 데이터 파이프라인

```
data-src/*.csv ──┬─ tools/convert.mjs (npm run generate:csv) ─→ data/*.json ─→ client (import)
                 └─ tools/cargo-build-data.rs (server/build.rs) ─→ data/*.json ─→ server (serde)
```

CSV는 `id` 컬럼으로 키가 잡힌 JSON이 된다. 빈 칸은 키 자체가 생략되므로 Rust 쪽은
`Option`/`#[serde(default)]`, TS 쪽은 `?`로 받는다. 값은 `true`/`false`면 bool,
숫자로 파싱되면 number, 나머지는 string으로 추론된다 — **CSV에 콤마를 넣지 말 것**
(파서가 단순 `split(',')`이다). 여러 값이 필요한 컬럼은 `|` 또는 세미콜론 구분을 쓴다.

### 포트

| 포트 | 서비스 |
|------|--------|
| 10004 | 클라이언트 (Vite dev) |
| 10005 | GLB 에디터 (`tools/glb-editor`) |
| 10006 | 서버 WebSocket (기본 127.0.0.1 바인드) |
| 10007 | 서버 REST: terrain / housing / NPC (기본 127.0.0.1 바인드) |

Vite dev 서버가 `/ws → 10006`, `/api → 10007`로 프록시한다(`client/vite.config.ts`).
별도 프록시를 띄울 필요가 없다. 다른 기기에서 붙어야 할 때만 `--bind 0.0.0.0`을 쓰되,
그 경로에는 TLS도 프록시도 없다.

---

## 5. 개발 루프

| 무엇을 고쳤나 | 무엇을 다시 돌리나 |
|---------------|--------------------|
| `server/` | 서버 watch가 알아서 재시작 |
| `shared/` | 서버 watch + **WASM 재빌드**(터미널 2) 둘 다 |
| `client/` | Vite HMR — 아무것도 안 해도 됨 |
| `data-src/*.csv` | 서버는 watch가 재빌드, 클라이언트는 `npm run generate:csv` |
| 새 GLB/사운드 | `bash tools/fetch-assets.sh` 후 브라우저 새로고침 |
| `assets.lock` | `bash tools/fetch-assets.sh` |

편의 명령:

```bash
npm --prefix client run generate:data   # CSV + 애니메이션 타이밍만 다시 생성
cargo run -p onlinerpg-server -- --help # 서버 CLI 플래그
RUST_LOG=debug cargo run -p onlinerpg-server   # 스폰/전투 주사위까지 로그
```

인게임 관리자 명령(`ADMIN_EMAILS`에 등록된 계정): `/notice`, `/kick`, `/mute`,
`/summon`, `/goto`, `/ban`. 맵 에디터는 로그인 토큰으로 REST 쓰기를 한다.

---

## 6. 커밋 전 검증 (CI와 동일)

작업 중에 매번 돌리지 말고, **커밋 직전에 한 번** 돌린다.

```bash
# Rust — 저장소 루트
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

# 클라이언트 — client/
npm run build:wasm
npm test
npm run check
npm run lint
npm run format:check      # 깨지면 npm run format
```

Rust만 고쳤으면 Rust 셋, 클라이언트만 고쳤으면 클라이언트 셋이면 된다.
`shared/`를 고쳤으면 **양쪽 다** 필요하다(WASM 경유로 클라이언트에 들어간다).

`/preflight` 스킬이 변경 파일을 보고 필요한 것만 골라서 돌려준다.

### 서버가 실제로 뜨는지 (`tools/smoke-local.sh`)

단위 테스트가 잡지 못하는 것들이 있다 — **부팅 시점 assert**(던전 보스 플래그,
`weaponTier` 범위, `sizeMult` 파싱, 참조된 디버프·아이템 존재)와 **DB 마이그레이션**은
서버를 실제로 띄워야만 돈다.

```bash
cargo build --release -p onlinerpg-server
tools/smoke-local.sh
```

빈 상태 디렉터리에 서버를 띄워 마이그레이션을 전부 새로 돌리고, 스키마를 확인하고,
지형 API가 **베이크된 타일**을 주는지(없으면 평평한 기본값이 오므로 바이트 수로는
구분되지 않는다), WebSocket 업그레이드가 되는지, 그리고 **같은 DB에 다시 띄워도**
마이그레이션이 멱등인지 본다. 도커 스택 전체를 검사하는 짝은
`tools/smoke-compose.sh`다.

월드가 실제로 렌더링되는지는 여전히 사람이 봐야 한다 (`game-login` 스킬).

---

## 7. 자주 하는 작업 레시피

각 항목의 자세한 절차는 대응하는 스킬(§9)에 있다.

### 몬스터 추가
1. `data-src/monsters.csv`에 행 추가. 모델은 `client/public/models/monsters/*.glb`,
   애니 클립 이름은 그 GLB 안의 실제 클립명과 일치해야 한다.
2. 지상 스폰은 `data-src/world.json`의 `ambientSpawns`에 `monsterType`을 추가한다 —
   몬스터는 플레이어를 따라다니며 스폰되고, 인당 상한은 `maxMonstersPerPlayer`,
   게이트는 몬스터 자신의 레벨이다. **존 파일(`data/terrain/zones/`)의 `monsterSpawns`
   사각형은 서버가 읽지 않는다**(맵 에디터 전용 레거시, [ZONE_SYSTEM.md](ZONE_SYSTEM.md)).
   던전 스폰은 `dungeonMinDepth`/`dungeonMaxDepth`/`dungeonWeight` 컬럼으로 붙는다.
3. `npm --prefix client run generate:monster-clips`로 공격 클립 타이밍을 다시 측정.
4. 새 에셋이면 `doc/assets/`에 출처·라이선스를 기록한다.

### 아이템 추가
1. `data-src/items.csv`에 행 추가 — 아이콘은 `client/public/items/`,
   월드 모델은 `client/public/models/objects/`.
2. 무게(`weight`), `equipSlot`, `basePrice`, 드랍 경로(`chestTier`/`chestChance`,
   `data-src/world_drop.csv`, 상인 `catalog`) 중 필요한 것만 채운다.
3. 아이콘/모델 제작은 `blender-item-asset` 스킬이 스케일·원점·512² 텍스처·
   128×128 아이콘까지 처리한다.

### WS 프로토콜 메시지 추가
1. `shared/src/messages.rs`의 `ClientMessage` 또는 `ServerMessage`에 variant 추가.
2. 서버: `server/src/connection.rs`에서 수신 분기, 상태 전이는 `game_state/`.
3. 클라이언트: `client/src/lib/network/networkTypes.ts`에 타입,
   `messageHandlers.ts`에 `case`, 송신은 `socket.ts`.
4. 직렬화는 MessagePack이다. **필드 추가는 하위호환이 아니다** — 서버와 클라이언트를
   같이 배포한다.

### 던전 추가
`data-src/dungeons.csv`에 입구 좌표·층수·보스·상자 티어를 넣는다.
지오메트리는 하우징 절차적 지오메트리를 재사용한다([doc/HOUSING_SYSTEM.md](HOUSING_SYSTEM.md)).

### UI 패널 추가
`client/src/lib/components/`에 Svelte 컴포넌트, 상태는 `lib/stores/`에 별도 store.
겹치는 오버레이는 `overlayStack.ts`에 등록해야 ESC 처리와 z-순서가 맞는다.

---

### 상태 백업과 복구 (`tools/backup-state.sh` · `restore-state.sh`)

```bash
tools/backup-state.sh /var/lib/onlinerpg /var/backups/onlinerpg   # 서버를 세우지 않는다
tools/restore-state.sh /var/backups/onlinerpg/<타임스탬프> /var/lib/onlinerpg
```

**상태는 DB만이 아니다.** 플레이어가 지은 집은 `housing/`의 JSON 파일이고,
공지는 `announcements/`, 봇 토큰은 `npc_token`이다. DB만 받아 두면 복구했을 때
**집이 전부 사라지고 헤드리스 에이전트가 로그인하지 못한다.** 두 스크립트는 넷을
같이 다룬다.

- 백업은 `VACUUM INTO`를 쓴다 — SQLite가 스스로 일관된 스냅숏을 뜨므로 **서버를
  멈출 필요가 없고**, 파일을 복사할 때처럼 트랜잭션 중간을 잡을 수 없다.
- 백업은 뜬 직후 **다시 열어 `integrity_check`와 캐릭터 수를 확인한다.** 열어 본
  적 없는 백업은 백업이 아니라 추측이다.
- 복구는 **서버가 떠 있으면 거절한다.** 판정은 `/proc/<pid>/exe`로 한다 —
  `pgrep -f`는 빌드나 편집기까지 잡고, `pgrep -x`는 리눅스가 프로세스 이름을
  15자로 자르는 탓에 `onlinerpg-server`(16자)를 **영영 못 잡는다**(조용히 통과하는
  쪽이 더 위험하다).
- 복구는 기존 상태를 지우지 않고 `replaced-<타임스탬프>/`로 옮긴다. 잘못된 백업을
  복구하는 것도 되돌릴 수 있어야 한다.

`tools/smoke-local.sh`가 라이브 백업과 "떠 있으면 거절"을 매번 확인한다.

## 8. 성능 기준선 — 동시 접속 5,000명

이 프로젝트는 **동시 5,000명에서 문제가 없어야 한다**. 새 코드를 넣기 전에 다음을 확인한다.

- **서버 루프에 O(플레이어²)를 만들지 않는다.** 브로드캐스트는 관심 영역(AOI)으로
  잘라서 보낸다. 전체 플레이어를 훑는 매 틱 순회는 금지.
- **틱 주기를 지킨다.** 이동은 5 Hz다. 새 주기 작업은 기존 틱에 얹거나 더 낮은
  주기를 쓴다.
- **핫 경로에서 잠금을 오래 쥐지 않는다.** 락 안에서 IO·직렬화·할당을 하지 않는다.
- **메시지당 바이트를 센다.** 5,000명 × 초당 N 메시지가 그대로 대역폭이다.
  전체 스냅샷보다 델타를 보낸다.
- **디스크 저장은 배치로.** 캐릭터/인벤 저장은 기존 배치 세이브에 합류시킨다.
- **클라이언트 60fps**: 드로우콜·머티리얼 인스턴스·GLB 캐시(`gltfCache.ts`)를 확인.
  씬에 매 프레임 새 오브젝트를 만들지 않는다.

측정 없는 최적화는 하지 않는다 — 실제 조사 사례와 판단 근거는
[doc/RUNTIME_PERFORMANCE.md](RUNTIME_PERFORMANCE.md), 로딩 쪽은
[doc/LOADING_OPTIMIZATION.md](LOADING_OPTIMIZATION.md)에 있다.

---

## 9. 에이전트 스킬 (`.claude/skills/`)

| 스킬 | 언제 |
|------|------|
| `dev-bootstrap` | 새 클론 세팅, "환경이 안 돌아간다" 진단 |
| `dev-run` | 서버·WASM watch·클라이언트를 정해진 순서로 띄우기 |
| `preflight` | 커밋 직전 CI 동등 검증(변경 파일 기준으로 선별) |
| `add-monster` | 몬스터 정의부터 스폰·클립 타이밍까지 |
| `add-item` | 아이템 정의부터 아이콘·드랍 경로까지 |
| `add-protocol-message` | shared → server → client 3면 동시 수정 |
| `blender-item-asset` | GLB 임포트·스케일·아이콘 렌더 (기존) |
| `blender-fix-armature-scale` | 아마추어 스케일 교정 (기존) |
| `game-login` | 실제 Chrome으로 로그인해 인게임 확인 (기존) |
| `/deploy`, `/compose-texture` | 커맨드 (`.claude/commands/`) |

---

## 10. 트러블슈팅

| 증상 | 원인 / 처방 |
|------|-------------|
| 월드가 검고 지형 API가 404 | 지형 미베이크 → §3-4 |
| 캐릭터가 허공에 서 있거나 지형 경계에서 뚝 끊김 | 베이크 리전 범위 밖. 범위를 넓혀 다시 베이크 |
| `Cannot find module '../wasm'` / WASM import 실패 | `npm --prefix client run build:wasm` |
| `data/monsters.json` 없음 | `npm --prefix client run generate:csv` |
| 모델이 안 보이고 404 | `bash tools/fetch-assets.sh` (`assets.lock` 변경 후 재실행) |
| 로그인 버튼이 에러 | `VITE_GOOGLE_CLIENT_ID`(클라) / `GOOGLE_CLIENT_ID`(서버)가 같은 Web ID인지 |
| 맵 에디터 저장이 403 | `ADMIN_EMAILS`에 로그인 계정이 없음 |
| 클라이언트만 고쳤는데 서버 동작이 그대로 | `shared/` 변경분이 WASM에 반영 안 됨 → 터미널 2 확인 |
| clippy가 CI에서만 실패 | 로컬에 `--locked`와 `--all-targets`를 빼먹었는지 확인 |
| 봇/NPC가 안 붙음 | `data/npc_token` 값과 `agent-client/data/config.toml`이 일치하는지 |

---

## 11. 문서 지도

**월드/지형** [WORLD_BUILDING](WORLD_BUILDING.md) · [MAP_DESIGN](MAP_DESIGN.md) ·
[TERRAIN_GENERATION](TERRAIN_GENERATION.md) · [RIVER_SYSTEM](RIVER_SYSTEM.md) ·
[WATER_SYSTEM](WATER_SYSTEM.md) · [VEGETATION_SYSTEM](VEGETATION_SYSTEM.md) ·
[ZONE_SYSTEM](ZONE_SYSTEM.md) · [SPLATMAP_V2](SPLATMAP_V2.md)

**게임플레이** [COMBAT](COMBAT.md) · [ENCHANT](ENCHANT.md) · [DEBUFF](DEBUFF.md) ·
[ECONOMY](ECONOMY.md) · [HOUSING_SYSTEM](HOUSING_SYSTEM.md) · [HUNGER](HUNGER.md) ·
[FISHING](FISHING.md) · [GATHERING](GATHERING.md) · [ITEM_TIERS](ITEM_TIERS.md) ·
[NPC_MONSTER_AI](NPC_MONSTER_AI.md) · [ANIMATION](ANIMATION.md)

**엔진/성능** [RUNTIME_PERFORMANCE](RUNTIME_PERFORMANCE.md) · [LOADING_OPTIMIZATION](LOADING_OPTIMIZATION.md)

**에셋/에이전트** [ASSETS](ASSETS.md) · [DESIGN_DIRECTION](DESIGN_DIRECTION.md) ·
[AGENT_CLIENT](AGENT_CLIENT.md) · [AGENT_CLIENT_QUICKSTART](AGENT_CLIENT_QUICKSTART.md) ·
[AGENT_MANAGER](AGENT_MANAGER.md) · [REMOTE_AGENT_CLIENT](REMOTE_AGENT_CLIENT.md)

**구조** [ARCHITECTURE](ARCHITECTURE.md) — 크레이트·런타임·프로토콜·데이터 파이프라인 상세 구성도

**개발 계획** [DEVELOPMENT_MASTER_PLAN](DEVELOPMENT_MASTER_PLAN.md) 착수 순서·게이트·마일스톤 ·
[ragnarok/13_IMPLEMENTATION_DIRECTION](ragnarok/13_IMPLEMENTATION_DIRECTION.md) 항목별 손댈 파일·스키마·검증

**설계 레퍼런스** [Ragnarok Online 시스템 레퍼런스](ragnarok/README.md) —
RO 시스템 정리 + [갭 분석](ragnarok/09_OPENMMO_GAP_ANALYSIS.md) + [구현 로드맵](ragnarok/10_IMPLEMENTATION_ROADMAP.md)

**작업 목록** [TODO](TODO.md) · [devlog](devlog/README.md)

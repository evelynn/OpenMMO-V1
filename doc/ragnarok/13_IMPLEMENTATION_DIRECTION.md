# 13. 구현 방향서 — 설계서를 지금의 솔루션에 어떻게 넣는가

[09_OPENMMO_GAP_ANALYSIS](09_OPENMMO_GAP_ANALYSIS.md)가 *무엇을* 만들지 정했고,
[10_IMPLEMENTATION_ROADMAP](10_IMPLEMENTATION_ROADMAP.md)이 *어떤 파일을* 훑었으며,
[DEVELOPMENT_MASTER_PLAN](../DEVELOPMENT_MASTER_PLAN.md)이 *어떤 순서로*를 정한다.
이 문서는 **현재 코드베이스의 어느 함수·어느 컬럼에 붙는가**를 항목마다 적는다.
항목 번호(`IMP-x.y`)는 상위 계획에서 참조하기 위한 고정 ID다.

---

## 0. 모든 방향을 규정하는 세 제약

아래 30개 항목의 설계가 RO 원본과 갈라지는 지점은 거의 전부 이 셋 중 하나에서 나온다.

| # | 제약 | 구현에 미치는 영향 |
|---|------|--------------------|
| **(a)** | **능력치 3~18, 4d6-drop-lowest, 총합 72, 생성 시 확정** (`shared/src/character.rs`, [doc/COMBAT.md](../COMBAT.md)) | 스탯을 인자로 쓰는 모든 공식은 √·1~130 스케일이 아니라 **d20 어빌리티 모디파이어**(`server/src/game/combat.rs`의 `ability_modifier`, `(stat-10)/2`) 위에서 다시 그린다. 성장형 스탯 배분(09 #27)은 존재하지 않으므로 "스탯을 더 올려 뚫는다"는 해법을 설계에 넣을 수 없다 — 장비·스킬·소비 아이템만이 조절 손잡이다. |
| **(b)** | **이음매 없는 32km 월드 + 존 기반 규칙** (`shared/src/world.rs`, [doc/ZONE_SYSTEM.md](../ZONE_SYSTEM.md), [doc/MAP_DESIGN.md](../MAP_DESIGN.md)) | RO가 "맵"에 붙이던 것(정원·레벨대·EXP 보정·워프)은 맵이 없으므로 **좌표 사각형 또는 CSV 좌표 행**에 붙는다. 층은 정수 인덱스 하나로 통합되어 있다 — 하우징 0..=3(`shared/src/housing.rs:11`), 던전 depth는 `DUNGEON_FLOOR_INDEX_BASE`(=4)부터. 위치를 다루는 모든 신규 기능은 X축 실린더 랩(`wrap_world_x`)과 층 일치 검사를 통과해야 한다. |
| **(c)** | **5,000 동접 + 에이전트-인간 동등성** ([doc/REMOTE_AGENT_CLIENT.md](../REMOTE_AGENT_CLIENT.md), [doc/RUNTIME_PERFORMANCE.md](../RUNTIME_PERFORMANCE.md)) | 새 상태는 캐릭터당 크기 × 5,000으로 곱해서 본다. 새 브로드캐스트는 `send_direct_message_to_players_within_position`(`server/src/game_state/player.rs:264`, 반경 43m + 층 일치)로 자르고, 전체 스냅샷 대신 델타를 보낸다. 새 디스크 쓰기 경로 대신 `flush_dirty_saves`(`player.rs:644`) → `AuthService::save_batch`(`server/src/auth.rs:1266`)에 합류한다. 그리고 **사람이 UI로 할 수 있는 모든 신규 행동은 `AgentAction`(`agent-client/src/driver/action.rs:13`)에도 있어야 한다** — 없으면 봇 전용/사람 전용 경로가 생겨 동등성이 깨진다. |

세 제약은 각 항목의 **성능** 절과 **구현 방향** 절에서 반복해서 근거로 쓰인다.
새 항목을 추가할 때도 이 셋을 먼저 통과시킨다.

---

# Phase 0 — 선결 (코드 이전)

| ID | 항목 | 09 # | 10 § |
|----|------|------|------|
| IMP-0.1 | 되돌릴 수 없는 결정 확정 | 09 §4-3 | — |
| IMP-0.2 | `GuardUpdated` → `EffectiveStats` 일반화 | — (doc/TODO.md) | — |

---

### IMP-0.1. 되돌릴 수 없는 결정 확정

| | |
|---|---|
| 결정 | 채택 (09 §4의 3번 기준: "나중에 못 넣는 것") |
| 난이도 | 소 (문서 작업, 코드 없음) |
| 선행 조건 | 없음 |
| 프로토콜 변경 | 없음 |
| 저장 스키마 변경 | 없음 |

**손댈 파일** — 이 문서뿐이다. 아래 네 결정의 **수치**를 해당 항목 절에 박아 넣고,
[DEVELOPMENT_MASTER_PLAN](../DEVELOPMENT_MASTER_PLAN.md) §8 표의 "확정 시점" 칸을 갱신한다.

| 결정 | 어디에 박히나 | **확정값** |
|------|---------------|------------|
| 일일 한도 | IMP-2.6 | 보드당 **일일 3회**, 계정이 아니라 **캐릭터** 단위. 리셋은 서버 시각 기준 매일 00:00 UTC (`DAILY_RESET_HOUR_UTC = 0`, `QUEST_DAILY_LIMIT = 3`) |
| 인스턴스 개인 쿨다운 | IMP-4.2 | **입장 시점**에 입장자 개인에게 부여. 파티 단위 아님 (`INSTANCE_COOLDOWN_SECS`, 첫 던전은 `21_600` = 6시간) |
| 거래 수수료는 소각 | IMP-3.5 | 임계 **10,000c 초과분의 5%**, 전액 소각 — NPC 수입으로 돌리지 않는다 (`TRADE_FEE_THRESHOLD = 10_000`, `TRADE_FEE_PCT = 5`) |
| 창고 슬롯 상한 | IMP-2.3 | `STORAGE_SLOTS = 120` — SPK-2는 이 값을 **검증만** 한다 |

> **확정 상태**: 네 값 모두 IMP-0.1에서 확정됐다. 구현 PR은 이 숫자를 상수로 옮겨
> 적기만 하고, 바꾸려면 이 표를 먼저 고친 뒤 그 PR에서 근거를 남긴다.

**구현 방향**
코드가 없는 항목을 별도 ID로 세운 이유는 하나다 — 이 넷은 **출시 후에 바꾸면 플레이어가
손해를 보는 방향으로만 바뀐다.** 한도는 나중에 붙이면 너프, 쿨다운은 개인화하는 순간
파티가 손해, 수수료는 나중에 붙이면 증세, 슬롯 상한은 낮추면 몰수다. 그래서 해당 항목을
구현하기 전에 숫자를 문서에 먼저 고정하고, 구현 PR은 그 숫자를 옮겨 적기만 한다.
숫자를 바꾸려면 이 절을 먼저 고친다.

**데이터 스키마** — 없음.
**마이그레이션** — 없음.
**검증** — 위 네 항목 절에 수치가 상수 이름과 함께 적혀 있으면 완료.
**성능** — 해당 없음.

---

### IMP-0.2. `GuardUpdated` → `EffectiveStats` 일반화

| | |
|---|---|
| 결정 | 채택 — 유지보수자 백로그(`doc/TODO.md:150`)와 이 문서의 여러 항목이 같은 그릇을 필요로 한다 |
| 난이도 | 중 |
| 선행 조건 | 없음 |
| 프로토콜 변경 | 있음 — `ServerMessage::GuardUpdated`를 `EffectiveStats`로 확장, `PROTOCOL_VERSION` +1 |
| 저장 스키마 변경 | 없음 (파생값만 전송) |

**손댈 파일**
- `shared/src/messages.rs:1060` — `GuardUpdated { guard }`를 `EffectiveStats { guard, attributes, .. }`로.
  기존 variant를 남기지 않는다(하위호환이 없으므로 유지 비용만 남는다).
- `server/src/game_state/inventory.rs:281` · `server/src/connection.rs:1151` — 두 송신 지점.
- `server/src/game_state/combat.rs:193` `effective_guard` — 능력치 보정까지 함께 계산하도록 확장.
- `client/src/lib/stores/inventoryStore.ts:22` · `client/src/lib/network/messageHandlers.ts:1233` — 수신·저장.
- `client/src/lib/components/CharacterPanel.svelte` — 시트에 **보정된** 능력치 표시.

**구현 방향**
지금 서버는 장비·디버프가 반영된 능력치를 **자기만 알고** 있다. `gold_ring`의 CHA +1은
흥정 계산에는 들어가지만 캐릭터 시트는 base만 보여주므로, 플레이어는 장비 효과를 눈으로
확인할 방법이 없다. `guard` 하나만 내려주던 메시지를 "서버가 계산한 최종 능력치" 묶음으로
넓히면 이 구멍이 닫히고, 이후 항목들이 같은 경로를 재사용한다.
**주의: 이 항목은 게이트가 아니다.** IMP-1.3(디버프 저항)과 IMP-1.5(크기 축)는 서버에서만
판정하므로 이 메시지 없이도 완결된다. 다만 두 항목이 들어간 뒤 이 메시지를 넓히면
"디버프로 낮아진 저항치"를 시트에 표시할 수 있으므로, 먼저 하면 표시 작업이 공짜가 된다.

**데이터 스키마** — 없음.
**마이그레이션** — 없음. 클라이언트는 첫 메시지 도착 전까지 `null`을 그대로 유지한다.
**검증** — 반지 착·탈 시 시트의 CHA가 즉시 바뀌고, 흥정 결과와 일치하는지 인게임 확인.
`doc/TODO.md:150` 항목 체크.
**성능** — 메시지 크기가 몇 바이트 늘 뿐이고 빈도는 그대로다(장비 변경 시 1회, 브로드캐스트 아님).

---

# Phase 1 — 컬럼 하나로 끝나는 것들

| ID | 항목 | 09 # | 10 § |
|----|------|------|------|
| IMP-1.1 | 레벨 차 EXP 페널티 | 1 | 1.1 |
| IMP-1.2 | 보스 프로토콜 | 7 | 1.2 |
| IMP-1.3 | 디버프 저항 스탯 | 11 | 1.3 |
| IMP-1.4 | 루터 몬스터 | 8 | 1.4 |
| IMP-1.5 | 크기 축 | 16 | 1.5 |
| IMP-1.6 | 무기 티어 = 제련 리스크 등급 | 17 | — |
| IMP-1.7 | 채팅 접두사 규약 | 37 (12 §2) | — |

---

### IMP-1.1. 레벨 차 EXP 페널티

| | |
|---|---|
| 결정 | 채택 (09 #1) |
| 난이도 | 소 |
| 선행 조건 | 없음 |
| 프로토콜 변경 | 있음 — `ServerMessage::XpGained`에 `xp_mult_pct: u8` 추가, `PROTOCOL_VERSION` 29 → 30 |
| 저장 스키마 변경 | 없음 |

**손댈 파일**
- `shared/src/xp.rs` — `monster_xp`(:4) 옆에 `level_diff_mult_bp(player_level, monster_level) -> u32`(basis point) 추가. 기존 `monster_xp` 시그니처는 **그대로 둔다**(:117~:138의 4개 테스트가 그대로 살아남는다).
- `server/src/game_state/combat.rs` — :499의 `base_xp` 계산은 유지, :507 `party_xp_share` 뒤 `grant_monster_kill_xp`(:541)에 `monster_level: u8`을 넘긴다.
- `shared/src/messages.rs` — `ServerMessage::XpGained`에 `xp_mult_pct: u8` 추가(끝에 append). 감쇠(<100)와 보너스(>100)를 한 필드로 표현하므로 `penalty_pct`가 아니라 배율 이름을 쓴다.
- `shared/src/lib.rs:78` — `PROTOCOL_VERSION` 증가 + 상단 체인지로그에 `/// v30:` 한 줄.
- `client/src/lib/network/messageHandlers.ts` — `XpGained` case에서 감쇠율 표시.
- `agent-client/src/driver/prompt.rs:164` `format_event` — `[Xp] ... (감쇠 40%)`.

**구현 방향**
감쇠는 **파티 분배 이후, 수령자별로** 적용한다. 수령자마다 레벨이 다르므로
`grant_monster_kill_xp(&[PlayerId], u32)`의 "전원 같은 금액" 전제가 깨진다. 다만 시그니처를
`&[(PlayerId, u32)]`로 바꿀 필요는 없다 — 이 함수는 이미 `player_characters` 쓰기 락 안에서
수령자별 `old_xp`를 읽으므로, 그 자리에서 `xp::level_from_xp(old_xp)`로 레벨을 얻어
`share × level_diff_mult_bp(level, monster_level) / 10_000`을 적용하면 **락을 한 번도 더
잡지 않는다**. 이게 "레벨을 미리 모아서 넘긴다"보다 나은 이유는 `players` 맵을 추가로
읽지 않아도 되기 때문이다(regen 틱이 두 락을 반대 순서로 잡는 제약을 건드리지 않는다).
곡선은 레벨 대역이 짧다는 점(`xp_for_level`은 레벨당 2배)을 감안해 완만하게 시작한다:
`|diff| ≤ 2` 100%, 플레이어가 높으면 초과 1레벨당 −10%p(바닥 10%), 플레이어가 낮으면
1레벨당 +5%p(천장 120%). 감쇠는 `party_xp_share` **밖**에 있으므로 파티 불변식
(`party_share_never_beats_soloing`, `shared/src/xp.rs:204`)은 손대지 않는다.

**데이터 스키마** — 없음. 곡선 상수는 `shared/src/xp.rs`에 `pub const` 6개
(`LEVEL_DIFF_BASE_BP = 10_000`, `LEVEL_DIFF_FREE_BAND = 2`,
`LEVEL_DIFF_DECAY_BP_PER_LEVEL = 1_000`, `LEVEL_DIFF_FLOOR_BP = 1_000`,
`LEVEL_DIFF_BONUS_BP_PER_LEVEL = 500`, `LEVEL_DIFF_CEIL_BP = 12_000`).
공개 함수는 `level_diff_mult_bp` · `apply_level_diff`(1 XP 바닥 포함) ·
`level_diff_mult_pct`(와이어·표시용).

**마이그레이션** — 없음. 누적 XP는 그대로이고 획득 속도만 바뀐다.
단, 저레벨 몹 파밍 중이던 캐릭터는 체감 수급이 급락하므로 배포 공지 대상이다.

**검증**
- `shared/src/xp.rs` 테스트: `|diff| ≤ 2` 무감쇠 / 바닥 10% 도달 / 천장 120% / 감쇠 후에도
  `party_xp_share ≤ solo` 유지.
- 인게임: 레벨 9 캐릭터로 `kobold`(level 1)와 `troll`(level 9)를 각각 잡아
  XP 로그의 감쇠율이 각각 바닥/100%인지 확인.

**성능** — 곱셈 하나. 새 락·새 순회·새 메시지 없음(`XpGained`는 이미 처치당 1회 전송).
`penalty_pct`는 u8 1바이트이며 이 메시지는 브로드캐스트가 아니라 수령자 직행이다.

---

### IMP-1.2. 보스 프로토콜

| | |
|---|---|
| 결정 | 채택 (09 #7) |
| 난이도 | 소 |
| 선행 조건 | 없음 (IMP-2.7 · IMP-2.8이 이 플래그를 소비) |
| 프로토콜 변경 | 없음 |
| 저장 스키마 변경 | 없음 |

**손댈 파일**
- `server/src/monster_defs.rs:14` `MonsterDefinition` — **`boss` 필드가 아예 없다.**
  `#[serde(default)] pub boss: bool` 추가 + `pub fn is_boss(&self) -> bool`.
- `server/src/monster_defs.rs` `MonsterDefs` — `pub fn boss_immune(&self, monster_type: &str) -> bool`.
- `server/src/dungeon_defs.rs` — `dungeons.csv`의 `boss` 몬스터가 존재하는지는 이미
  검사한다(:50). 여기에 **그 몬스터의 `boss` 컬럼이 `true`인지**도 `boss_immune`으로 assert 추가.
- `doc/DEBUFF.md` — 예외 규칙 한 줄.

> **개정 (IMP-1.2 착수 시)** — 두 가지를 고쳤다.
> 1. 진입점을 자유 함수 `boss_immune(monster_type)`가 아니라 **`MonsterDefs`의 메서드**로 둔다.
>    `MonsterDefs`는 `GameState`가 들고 있는 인스턴스이고(`MonsterDefs::load()`), `shared`의
>    던전 레지스트리와 달리 프로세스 전역이 아니다. 자유 함수로 두려면 전역을 새로 만들어야
>    하는데, 그것은 이 항목이 사려는 것(진입점 하나)보다 비싼 변경이다.
> 2. **`debuff.rs`에는 호출부를 추가하지 않는다.** `inflict_debuff`는 `PlayerId`를 받고,
>    이 절이 스스로 적었듯 몬스터를 대상으로 하는 상태 부여·강제 이동 경로는 아직 없다 —
>    넣을 호출부 자체가 존재하지 않는다. 대신 진입점의 **첫날 호출부는 부팅 검증**이다
>    (`dungeon_defs.rs`). 호출부 없는 `pub fn`은 `-D warnings`에서 죽은 코드로 잡히므로,
>    "나중에 붙일 자리"를 호출 없이 남겨 두는 선택지는 없다.

**구현 방향**
현 코드의 실상을 먼저 적는다. (1) `data-src/monsters.csv`에 `boss` 컬럼이 있고
`orc_boss` / `goblin_boss` / `ogre_boss`가 `true`지만, **서버는 이 컬럼을 읽지 않는다** —
`MonsterDefinition`에 필드가 없다. 읽는 곳은 클라이언트의 이름표 표시뿐이다
(`client/src/lib/data/monsterDefs.ts:54`). (2) 디버프는 `hunger` 맵에 `PlayerId`로만
붙는다 — **몬스터는 디버프를 받을 수 없다.** (3) 넉백은 코드베이스에 존재하지 않는다
(`grep -rn "knockback" --include=*.rs --include=*.ts` → 0건).
따라서 이 항목이 지금 실제로 만드는 것은 **면역 규칙 자체가 아니라 그 규칙이 붙을
단일 진입점**이다: `MonsterDefs::boss_immune(&self, monster_type: &str) -> bool`을
`server/src/monster_defs.rs`에 두고, 앞으로 몬스터를 대상으로 하는 상태 부여·강제 이동
경로는 예외 없이 여기를 지나게 한다. 09 문서가 "코드 변경 최소"라고 본 것은 맞지만
그것은 **지금 심어야 나중에 싸다**는 뜻이지 오늘 눈에 보이는 효과가 있다는 뜻이 아니다.
오늘 즉시 얻는 것은 서버가 `boss`를 읽게 되는 것과, `dungeons.csv`의 보스 지정이
실제 보스 몬스터인지 부팅 시 검증되는 것 두 가지다.

**데이터 스키마** — 컬럼 추가 없음. `monsters.csv`의 기존 `boss` 컬럼이 처음으로
서버에서 소비될 뿐이다(빈 칸 = false).

**마이그레이션** — 없음.

**검증**
- `server/src/monster_defs.rs` 테스트: `orc_boss.is_boss()` true, `orc.is_boss()` false.
- 부팅 검증: `dungeons.csv`의 `boss`를 일반 몬스터로 바꾸면 서버가 기동에 실패하는지.
- 인게임: 보스 처치까지 진행해 이름표와 서버 로그의 보스 판정이 일치하는지.

**성능** — 정적 테이블 조회 1회. `MonsterDefs`의 `Arc<HashMap>`(`server/src/monster_defs.rs:97`, `MonsterDefs::load()`에서 프로세스당 1회 파싱)에서 O(1).

---

### IMP-1.3. 디버프 저항 스탯

| | |
|---|---|
| 결정 | 채택 (09 #11) |
| 난이도 | 소 |
| 선행 조건 | 없음 |
| 프로토콜 변경 | 없음 |
| 저장 스키마 변경 | 없음 |

**손댈 파일**
- `data-src/debuffs.csv` — `resistStat`, `resistK` 두 컬럼 추가.
- `server/src/debuff_defs.rs:12` `DebuffDef` — `resist_stat: Option<String>`,
  `resist_k: u32`(`#[serde(default = "default_resist_k")]`, 기본 8) 추가.
- `server/src/game_state/debuff.rs:72` `inflict_debuff` — 확률 보정.
- `doc/DEBUFF.md` — 표 갱신.

**구현 방향**
`inflict_debuff`는 현재 `def.chance`를 **어떤 await보다도 먼저** 굴린다
(`thread_rng`가 `!Send`이기 때문 — 주석에 명시되어 있다). 저항 값은
`player_characters` 읽기 락에서 와야 하므로 순서를 **① 능력치 읽기(await) → ② 굴림
→ ③ `hunger` 쓰기 락**으로 바꾼다. 이 순서를 지키지 않으면 컴파일이 깨진다.
보정식은 제약 (a) 때문에 RO의 VIT/LUK 선형식을 그대로 쓸 수 없다. 3~18 스케일에서
의미 있는 유일한 정규화는 이미 전투 전체가 쓰는 어빌리티 모디파이어이므로
`server/src/game/combat.rs`의 `ability_modifier`를 재사용한다:

```
effective_chance = clamp(chance × (100 − ability_modifier(stat) × resistK) / 100, 0, 100)
```

`resistK = 8`이면 CON 18(mod +4) → 확률 −32%, CON 6(mod −2) → +16%.
`resistStat`이 비어 있으면 기존 고정 확률 그대로다 — 즉 **기존 두 디버프의 밸런스를
건드리지 않고 컬럼만 채워 나갈 수 있다**. 이것이 "스탯을 인자로 받는 새 공식"보다
`Option<String>` 컬럼을 고른 이유다.

**데이터 스키마**

| 컬럼 | 타입 | 빈 칸 | 의미 |
|------|------|-------|------|
| `resistStat` | `str`\|`dex`\|`con`\|`int`\|`wis`\|`cha` | 저항 없음 | 방어자의 어느 능력치가 깎는가 |
| `resistK` | 정수 | 8 | 모디파이어 1점당 확률 감소 %p |

```csv
id,name,chance,durationSecs,dps,moveMult,attackMult,carryMult,drainMult,blocksRegen,resistStat,resistK
food_poisoning,Food Poisoning,70,300,,0.6,0.6,0.6,4,true,con,8
bleed,Bleeding,35,8,1,,,,,true,con,5
```

**마이그레이션** — 없음. 컬럼 추가는 두 변환기(`tools/convert.mjs`,
`tools/cargo-build-data.rs`) 모두 헤더 기준이라 자동 반영되지만, **모든 행에 쉼표를
맞춰 넣어야 한다** — Rust 변환기는 필드 수 불일치에서 빌드를 실패시킨다.

**검증**
- `server/src/debuff_defs.rs` 테스트: `resistStat` 없는 행이 기본값으로 파싱되는지.
- `game_state/debuff.rs` 테스트: `inflict_debuff(force: Some(true))` 경로가 저항과
  무관히 부여되는지(기존 테스트 보존), 확률 계산 함수 단위 테스트로 CON 3/10/18 경계.
- 인게임: CON 최저/최고 캐릭터로 `gnoll`(hitDebuff=bleed)에게 각 20회 피격 후 부여 빈도 비교.

**성능** — 부여 시도당 `player_characters` 읽기 락 1회 추가. 부여 시도는 피격 이벤트에만
발생하고 5,000명 전체 순회가 아니다. 만약 훗날 초당 수천 건 규모가 되면 능력치를
`HungerData`에 캐시하는 것이 다음 수순이다(지금은 불필요).

---

### IMP-1.4. 루터 몬스터

| | |
|---|---|
| 결정 | 채택 (09 #8) |
| 난이도 | 중 |
| 선행 조건 | 없음 |
| 프로토콜 변경 | 있음 — `ClientMessage::MonsterPickupItem`, `ServerMessage::MonsterLootChanged`(선택), `PROTOCOL_VERSION` +1 |
| 저장 스키마 변경 | 없음 (몬스터가 든 아이템은 메모리 전용, 사망 시 바닥으로 반환) |

**손댈 파일**
- `data-src/monsters.csv` — `behavior`에 `looter` 값 추가(현재 `timid`/`brave`).
- `data-src/behavior_trees.json` — `looter` 트리 추가.
- `shared/src/monster_ai/tree.rs` / `behavior.rs` — `ground_item_in_range` 조건 노드,
  `move_to_ground_item` / `pick_up_ground_item` 액션 노드.
- `shared/src/monster_ai/command.rs:49` `AiCommand` — `PickUpItem { monster_id, instance_id }` 변형.
- `client/src/lib/managers/monsterManager.ts:724` `processAiCommands` — 새 분기 →
  `networkManager.sendMonsterPickupItem`. 인터페이스는 :47의 `AiCommand`도 확장.
- `server/src/connection.rs:733` — 디스패치 arm 추가.
- `server/src/game_state/inventory.rs` — `monster_pickup_item` 검증 + `monster_loot` 맵.
- `server/src/game_state/monster.rs` — `despawn_monsters` / `remove_monsters_by_owner`에서
  `monster_loot` 정리.
- `server/src/game_state/combat.rs` — 사망 처리(:446 부근)에서 들고 있던 것을
  `spawn_kill_loot_after_impact` 경로로 반환.

**구현 방향**
AI는 소유자 클라이언트에서 돌지만 **줍기·드랍은 100% 서버 검증**이다. 클라이언트는
"이 몬스터가 이 바닥 아이템을 줍겠다"는 요청만 보내고, 서버는 `pickup_item`
(`server/src/game_state/inventory.rs:1421`)과 동일한 게이트를 적용한다: 소유권
(`Monster::is_controllable_by`), 몬스터 위치와 아이템 위치의 거리, **층 일치**
(제약 (b) — 2층에서 떨어진 것은 2층에서만), 그리고 `ground_items` 쓰기 락 안에서 수량
재확인. 아이템은 인벤토리 시스템이 아니라 `monster_loot: Arc<RwLock<HashMap<String,
Vec<ItemInstance>>>>` 별도 맵에 담는다 — 몬스터에게 무게·장비 슬롯 개념을 주지 않기
위해서다. 사망 시 `loot_drop_position`으로 시체 주변에 흩뿌려 반환하고, despawn
경로에서도 반드시 맵을 지운다(안 지우면 세션당 누수).
`ServerMessage::MonsterLootChanged`는 선택 사항이다 — 몬스터가 아이템을 들고 있다는
시각적 표현이 없다면 `GroundItemRemoved`만으로 충분하고, 프로토콜 표면을 줄이는 쪽이
5,000명 관점에서 낫다.

**데이터 스키마**

```csv
id,...,behavior,...
gnoll,...,looter,...
```
`behavior` 값은 `data-src/behavior_trees.json`의 트리 이름과 정확히 일치해야 한다.

**마이그레이션** — 없음. 기존 몬스터는 `behavior`를 바꾸지 않는 한 영향 없음.

**검증**
- `shared/src/monster_ai` 단위 테스트: 사거리 안 바닥 아이템이 있을 때 `PickUpItem` 발행.
- `server/src/game_state/tests/` — 다른 층 아이템 줍기 거부, 소유자 아닌 클라이언트의
  요청 거부, 사망 시 반환 수량 일치, despawn 후 `monster_loot`가 비는지.
- 인게임: 아이템을 바닥에 버리고 루터 몬스터를 유인 → 줍는지, 잡으면 되찾는지,
  로그아웃으로 몬스터가 사라질 때 아이템이 증발하지 않는지.

**성능** — 몬스터당 소지 상한을 **4스택**으로 고정한다(초과 시 줍기 거부). 상한이
없으면 방치된 바닥 아이템이 한 마리에 몰려 사망 시 한 번에 수십 개를 스폰한다.
AI 탐색은 이미 소유자 클라이언트의 AOI 안에서만 일어나므로 서버 비용은 요청당
검증 한 번이며, 요청 자체는 `ConnectLimiter`가 아니라 몬스터 이동과 같은
소유자 신뢰 경로를 탄다 — 필요 시 몬스터당 줍기 쿨다운 2초를 추가한다.

---

### IMP-1.5. 크기 축

| | |
|---|---|
| 결정 | 채택(축소) (09 #16) |
| 난이도 | 소 |
| 선행 조건 | 없음 |
| 프로토콜 변경 | 없음 (크기는 `monsters.csv`에서 클라이언트도 직접 읽는다) |
| 저장 스키마 변경 | 없음 |

**손댈 파일**
- `data-src/monsters.csv` — `size` 컬럼 (`small`/`medium`/`large`, 빈 칸 = medium).
- `data-src/items.csv` — `sizeMult` 컬럼 (`"1.0|1.0|1.0"` 파이프 3연, 빈 칸 = 전부 1.0).
- `server/src/monster_defs.rs:14` — `size: MonsterSize`(`#[serde(default)]`).
- `server/src/item_defs.rs` — `size_mult: [f32; 3]`, 파싱은 로드 시 1회.
- `server/src/game/combat.rs` — `pub fn scale_damage(damage: u32, mult: f32) -> u32`(바닥 1).
- `server/src/game_state/combat.rs:316` `broadcast_player_attack` — :366~:377의 굴림 직후 적용.
- `client/src/lib/data/monsterDefs.ts` — `size?: string`, 이름표에 표시.
- `agent-client/src/state/world_state.rs:15` `format_world_state` — 몬스터 줄에 크기 표기.

> **개정 (IMP-1.5 착수 시)** — "이름표에 표시"는 **보스에만 걸린다.** 클라이언트에
> 이름표가 있는 것은 보스뿐이고(`client/src/lib/components/Monster.svelte`의 `isBoss`
> 분기), 일반 몬스터에 이름표를 새로 다는 것은 이 항목이 사려던 것보다 훨씬 큰 시각적
> 변경이다. 그래서 지금 사람이 크기를 읽을 수 있는 곳은 보스 이름표뿐이고, 에이전트는
> `format_world_state`에서 전부 본다 — **제약 (c)의 동등성이 아직 완전하지 않다.**
> 남은 절반은 일반 몬스터용 호버/타겟 UI가 생길 때 같이 붙인다. 그 UI는 이 항목의
> 범위가 아니므로 여기에 과제로만 남긴다.

**구현 방향**
곱은 **주사위 굴림 뒤**에 적용한다. `roll_attack`(`server/src/game/combat.rs:89`)은
순수 함수로 남기고, 호출부인 `broadcast_player_attack`에서 `scale_damage`를 한 번
통과시킨다. `roll_attack`에 인자를 하나 더 넣지 않는 이유는 몬스터→플레이어 공격
경로(`broadcast_monster_attack`, `combat.rs:651`)에는 크기 배율이 없기 때문이다 —
플레이어에게 크기 축을 주지 않는 것이 "축소 채택"의 실체다.
파이프 3연 문자열을 고른 이유는 컬럼 3개보다 CSV 폭이 좁고, `items.csv`가 이미
25컬럼이기 때문이다. 파싱 실패는 부팅 실패로 처리한다(이 프로젝트의 관성 —
`ItemEffect::parse`가 같은 방식).
에이전트가 크기를 볼 수 없으면 무기 선택이라는 의사결정에서 사람보다 불리해지므로
`format_world_state`에 노출하는 것은 선택이 아니라 제약 (c)의 요구다.

**데이터 스키마**

| 파일 | 컬럼 | 값 | 빈 칸 |
|------|------|-----|-------|
| `monsters.csv` | `size` | `small` / `medium` / `large` | `medium` |
| `items.csv` | `sizeMult` | `소\|중\|대` 실수 3연, 예 `1.25\|1.0\|0.75` | `1.0\|1.0\|1.0` |

```csv
# items.csv 발췌
dagger,Dagger,...,1.25|1.0|0.75
spear,Spear,...,0.75|1.0|1.25
```

**마이그레이션** — 없음. 빈 칸이 전부 중립값이라 데이터를 채우기 전까지 밸런스 무변화.

**검증**
- `server/src/game/combat.rs` 테스트: `scale_damage(1, 0.5) == 1`(바닥), 반올림 경계.
- `server/src/item_defs.rs` 테스트: 잘못된 `sizeMult`가 로드에서 패닉하는지.
- 인게임: 단검으로 `kobold`(small)와 `ogre`(large)를 각 20회 때려 평균 피해 차이 확인.

**성능** — 곱셈 1회. 정의 테이블은 프로세스당 1회만 파싱한다 (`server/src/monster_defs.rs` `MonsterDefs::load`, `server/src/item_defs.rs:255`의 `OnceLock`).

---

### IMP-1.6. 무기 티어 = 제련 리스크 등급

| | |
|---|---|
| 결정 | 채택(변형) (09 #17) |
| 난이도 | 소 |
| 선행 조건 | 없음 |
| 프로토콜 변경 | 없음 |
| 저장 스키마 변경 | 없음 (`enchant`는 이미 `character_items`에 있다) |

**손댈 파일**
- `data-src/items.csv` — **`weaponTier` 신규 컬럼** (1~5, 빈 칸 = 3).
- `server/src/item_defs.rs` — `weapon_tier: Option<u8>`.
- `server/src/game_state/inventory.rs:42` `enchant_success_bp` — 티어 오프셋 인자화.
- `doc/ENCHANT.md` — 사다리 표에 티어별 열 추가.

**구현 방향**
`rarityTier`를 재사용하지 않는다 — `server/src/item_defs.rs`에서 그 필드는
"Fish only(낚시 가중치·스킬 XP)"로 문서화되어 있고, 무기 티어를 얹으면 낚시 테이블이
오염된다. 새 컬럼이 옳다.
구현은 이미 저장소에 있는 트릭을 그대로 재사용한다: 방어구 사다리는
`armor_enchant_success_bp(e) = enchant_success_bp(e + 2)`
(`inventory.rs:58`)로 **사다리를 두 칸 미는 것**이 전부다. 무기 티어도 동일하게
`enchant_success_bp(enchant + (tier as i32 - 3))`로 표현한다 — 티어 1은 두 칸 안전,
티어 5는 두 칸 위험, 티어 3이 현행 곡선. 새 확률표를 만들지 않으므로
`ENCHANT_BP_SCALE`(:24)과 1% 바닥 규칙이 자동으로 유지되고,
기존 테스트(`enchant_success_ladder_halves_past_seven_with_one_percent_floor`, :1748)를
파라미터화해 재사용할 수 있다.

**데이터 스키마**

| 컬럼 | 값 | 빈 칸 | 효과 |
|------|-----|-------|------|
| `weaponTier` | 1~5 | 3 | 인챈트 사다리 오프셋 `tier − 3` (음수 = 안전, 양수 = 위험) |

```csv
id,name,...,weaponTier
dagger,Dagger,...,1
spear,Spear,...,3
```

**마이그레이션** — 없음. 이미 강화된 아이템의 `enchant` 값은 그대로이고,
**다음 강화 시도부터** 새 확률이 적용된다. 티어를 채우면 기존 고티어 무기의 실패
확률이 올라가므로 데이터 입력은 밸런스 결정이고 배포 공지 대상이다.

**검증**
- `inventory.rs` 테스트: 티어 1/3/5의 +5→+6 성공률이 각각 **100/75/25%**인지, 1% 바닥이
  모든 티어에서 유지되는지.
  > **개정 (IMP-1.6 착수 시)** — 원래 이 줄은 티어 5를 **50%**로 적었는데, 같은 절이
  > 정한 공식 `enchant_success_bp(enchant + tier − 3)`이 내놓는 값과 어긋난다.
  > 티어 5의 +5 시도는 `enchant_success_bp(7) = 2_500bp = 25%`다(50%는 `bp(6)`,
  > 즉 티어 4의 값이다). 공식이 정본이므로 기대값 쪽을 고쳤다 — 새 확률표를
  > 만들지 않는 것이 이 항목의 요점이다.
- 인게임: 티어 1 무기와 티어 5 무기에 주문서를 반복 사용해 파괴 시점 차이 체감.

**성능** — 없음. 강화는 플레이어 행동당 1회.

---

### IMP-1.7. 채팅 접두사 규약 (`%` 파티)

| | |
|---|---|
| 결정 | 채택 (12 §2) |
| 난이도 | 소 |
| 선행 조건 | 없음 (`$` 길드는 IMP-4.1 이후) |
| 프로토콜 변경 | 없음 — 기존 `ChatMessage` 본문 파싱만 바뀐다 |
| 저장 스키마 변경 | 없음 |

**손댈 파일**
- `shared/src/messages.rs` — `strip_command`(:207) 옆에
  `pub const PARTY_CHAT_PREFIX: char = '%'`와
  `pub fn split_channel_prefix(text: &str) -> (Option<ChatChannelHint>, &str)`.
- `server/src/game_state/chat.rs:297` `send_chat_message` — 접두사를 벗겨 파티 경로로 라우팅.
- `client/src/lib/components/ChatPanel.svelte` `sendMessage` — 한 줄 한정 채널 전환.
- `client/src/lib/stores/chatChannelStore.ts` — sticky 채널은 **바꾸지 않는다**.
- `agent-client/src/driver/action.rs:322` `ACTION_SPECS` — `say` 문서에 접두사 한 줄.

**구현 방향**
파싱은 반드시 `shared/`에 둔다. 클라이언트만 파싱하면 에이전트가 `%안녕`을 보냈을 때
서버가 그대로 주변 채팅으로 흘리므로 제약 (c) 위반이다. 서버가 최종 권위이고
클라이언트는 입력창 프리뷰(ghost preview)만 담당한다.
접두사는 **그 줄에만** 적용하고 sticky `chatChannel`은 건드리지 않는다 — 12 문서가
말하는 "실수 전송 통제"는 습관이 만드는 것이고, 접두사가 채널을 바꿔 버리면 다음
줄부터 반대 실수가 난다. 기존 `shouldRevertToSay`(빈 초안일 때만 되돌림)의 설계
의도와 같은 방향이다.
`$`(길드)는 IMP-4.1 전까지 파서에 자리만 만들고 시스템 메시지로 거절한다 — 나중에
접두사를 새로 배우게 하는 것보다 낫다.

**데이터 스키마** — 없음.

**마이그레이션** — 없음. 단, `%`로 시작하는 정상 발화가 파티로 새는 것을 막기 위해
`%%`를 리터럴 `%` 이스케이프로 정의하고 테스트한다.

**검증**
- `shared/src/messages.rs` 테스트: `%hi` → (Party, "hi"), `%%hi` → (None, "%hi"),
  `%` 단독 → (None, "%"), `/p hi`와 결과 동치.
- 인게임: 파티 상태에서 `%안녕` 전송 후 비파티원에게 보이지 않는지, 파티 없이
  보냈을 때 시스템 거절 메시지가 나오는지.

**성능** — 문자열 앞 1바이트 검사. 채팅 경로는 이미 존재.

---

# Phase 2 — 월드를 쓰게 만드는 것들

| ID | 항목 | 09 # | 10 § |
|----|------|------|------|
| IMP-2.1 | 우편함 | 36 (12 §4) | — (2.5의 선행) |
| IMP-2.2 | 세이브 포인트 + 리스폰 | 4 | 2.1(a) |
| IMP-2.3 | 창고 | 6 | 2.1(b) |
| IMP-2.4 | 유료 이동 + 던전 워프 불가 | 4·5 | 2.1(c) |
| IMP-2.5 | 헌팅 보드 반복 퀘스트 | 2 | 2.2 |
| IMP-2.6 | 일일 한도 + 고효율 | 3 | 2.2 |
| IMP-2.7 | 미니보스 | 9 | 2.3 |
| IMP-2.8 | MVP 기여도 보너스 | 10 | 2.4 |

IMP-2.2 / 2.3 / 2.4는 **하나의 도시 서비스 NPC**에 묶인다. NPC 자체는 새로 만들지
않는다 — `data-src/npcs.csv`에 행을 추가하고 `agent-client/data/npcs/<id>/`에
프롬프트를 두면 기존 에이전트 파이프라인이 그대로 태운다. 다만 세이브·창고·이동은
LLM의 재량이 아니라 **결정적 서버 판정**이어야 하므로, NPC는 대화 표면이고
실제 실행은 전용 `ClientMessage`가 담당한다(NPC가 응답을 잊어도 서비스는 동작해야 한다).

---

### IMP-2.1. 우편함

| | |
|---|---|
| 결정 | 채택 (12 §4 — 운영 안전장치) |
| 난이도 | 중 |
| 선행 조건 | 없음 |
| 프로토콜 변경 | 있음 — `OpenMailbox` / `ClaimMail` / `DeleteMail`, `MailList` / `MailUpdated` / `MailUnread` (v30 → **v31**) |
| 저장 스키마 변경 | 있음 — `mail`, `mail_items` 두 테이블 |

**손댈 파일**
- `server/src/auth.rs` — `ensure_mail_schema` 추가 후 `AuthService::new`의
  마이그레이션 블록에서 호출. **구현 결과 메서드는 6개**: `insert_mail`(상한 검사 포함),
  `load_mail`(읽음 표시까지), `load_one_mail`, `unread_mail_count`, `delete_mail`,
  `delete_expired_mail`. 페이로드는 `NewMail<'_>` 구조체로 묶었다 — 인자 7개짜리
  호출부는 읽히지 않는다. `character_id_of_name`도 함께 추가(관리자 명령이 오프라인
  캐릭터를 찾아야 한다). `AuthError::MailboxFull` 신설.
- `shared/src/messages.rs:232`/`:577` — 메시지 6종 + `MailSummary` 페이로드 구조체.
- `shared/src/lib.rs:78` — 버전 +1 및 체인지로그.
- `server/src/connection.rs:733` — 디스패치 arm 3개.
- `server/src/game_state/mail.rs` (신규) — `impl super::GameState` 블록 +
  `Letter` 구조체(발신자·제목·본문·골드·첨부). 블로킹 DB 호출은 `spawn_blocking`을
  직접 쓰지 않고 기존 `game_state::auth_db` 래퍼를 쓴다 — 같은 동작이고 에러 변환이
  한 곳에 모인다.
- `server/src/game_state/chat.rs` — **관리자 명령 `/mail <name> <message>`**
  (`AdminCommand::Mail`, `parse_admin_command`, `requires_admin` 경유). 마스터 플랜
  §5 row 11의 "운영 지급 경로"가 이것이고, 동시에 우편의 **첫 생산 호출부**다.
  이것이 없으면 배달 경로 전체가 죽은 코드로 남아 clippy `-D warnings`를 통과하지 못한다.
  이 명령을 위해 `send_chat_message`/`handle_admin_command`가 `&Arc<AuthService>`를
  받도록 넓혔고, 테스트 헬퍼 `make_test_auth`도 `Arc`를 반환한다.
- `server/src/game_state/mod.rs` — `mod mail;`.
- `client/src/lib/stores/mailStore.ts`(신규), `MailPanel.svelte`(신규),
  `GameHud.svelte` 마운트, `overlayStack.ts:24` `OVERLAYS`에 `mail` 등록(layer 0).
- `client/src/lib/network/messageHandlers.ts:284` — case 3개 + 세션 리셋에 `resetMailStore()`.
- `agent-client/src/driver/action.rs` — `AgentAction::CheckMail` / `ClaimMail` +
  `ACTION_SPECS` 항목, `msg_name`(`agent-client/src/main.rs:391`) arm, `format_event`,
  `classify_event`(신규 우편은 Routine).

**구현 방향**
12 문서의 판단대로 **콘텐츠가 아니라 운영 도구**로 먼저 짓는다. 그래서 최초 발신자는
플레이어가 아니라 시스템이다 — 플레이어 간 우편은 이 항목의 범위 밖으로 둔다(사기·
스팸·첨부 아이템 세탁이라는 별도 설계를 부른다).
저장은 **배치 세이브에 합류시키지 않는다.** 우편은 초당 수천 건이 아니라 보상 지급
시점에만 생기는 희소 이벤트이고, 크래시로 사라지면 안 되는 재화이므로
`tokio::task::spawn_blocking`으로 즉시 커밋한다. 대신 **틱에서 쓰지 않는다** —
5,000명 순회로 만료를 청소하는 대신 기존 `tick_buyback_expiry`(1시간 주기)에 얹어
`DELETE FROM mail WHERE expires_at < ?`를 한 문장으로 처리한다.
수령은 `stack_into_bag`(`server/src/game_state/inventory.rs:95`)과 무게 검사
(`max_carry_weight`, :237)를 통과해야 하고, **부분 수령을 허용하지 않는다** — 한 통을
통째로 받을 수 없으면 그대로 남긴다. 부분 수령을 허용하면 첨부 목록 상태를 우편에
저장해야 하고 그것이 곧 두 번째 인벤토리가 된다.
전송은 델타다: 접속 시 `MailUnread { count }`만 보내고, 패널을 열 때 `MailList`,
이후 변화는 `MailUpdated { mail_id, state }` 단건. 이것이 5,000명에게 우편함 전체를
밀지 않기 위한 유일한 이유다. `state`는 `MailState { Claimed, Deleted, ClaimBlocked }` —
`ClaimBlocked`는 **아무것도 움직이지 않았다**는 뜻이고 편지는 그대로 남는다(전부-아니면-전무
규칙의 클라이언트 쪽 표현). 새 편지가 도착하면 요약이 아니라 `MailUnread`를 다시 밀고,
목록은 플레이어가 패널을 열 때만 간다. `MailList` 직후에는 `MailUnread { count: 0 }`가
따라간다 — 목록을 읽는 것이 곧 읽음 처리이기 때문이다.

**데이터 스키마**

```sql
CREATE TABLE IF NOT EXISTS mail (
  id                   INTEGER PRIMARY KEY,
  recipient_character_id INTEGER NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
  sender               TEXT NOT NULL,          -- 'System' | NPC 이름
  subject              TEXT NOT NULL,
  body                 TEXT NOT NULL DEFAULT '',
  gold                 INTEGER NOT NULL DEFAULT 0,
  created_at           INTEGER NOT NULL,
  read_at              INTEGER,
  claimed_at           INTEGER,
  expires_at           INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_mail_recipient ON mail(recipient_character_id);
CREATE TABLE IF NOT EXISTS mail_items (
  mail_id      INTEGER NOT NULL REFERENCES mail(id) ON DELETE CASCADE,
  item_def_id  TEXT NOT NULL,
  quantity     INTEGER NOT NULL,
  enchant      INTEGER NOT NULL DEFAULT 0
);
```

상수: `MAX_MAILBOX: u16 = 30`(초과 시 `MailboxFull` + 로그),
`MAIL_TTL_SECS: i64 = 30 * 24 * 60 * 60`(30 실시간일. 초 단위로 두는 편이 `expires_at`
계산과 테스트 고정에 곧바로 쓰인다).

**마이그레이션** — 신규 테이블뿐. 기존 캐릭터는 빈 우편함으로 시작한다.
`ON DELETE CASCADE`가 `PRAGMA foreign_keys = ON`(`AuthService::new`)에 의존하므로
그 설정이 켜져 있는지 확인 후 병합할 것.

**검증** (구현 완료 — 8개 테스트)
- `server/src/auth.rs`: `mail_round_trips_with_its_attachments`,
  `a_full_mailbox_refuses_delivery`, `expired_mail_is_swept_and_live_mail_is_not`,
  `deleting_a_character_cascades_its_mail`(첨부 행까지 확인).
- `server/src/game_state/tests/mail_tests.rs`:
  `claiming_moves_gold_and_attachments_into_the_bag`,
  `an_overweight_claim_is_refused_and_the_mail_stays`(거부 시 인벤토리 메시지가
  나가지 않는 것까지 확인), `delivery_pushes_the_unread_badge_and_opening_clears_it`,
  `the_hourly_sweep_drops_expired_mail`.
- 인게임: 인벤토리를 가득 채운 상태에서 보상 지급 → 우편 도착 → 정리 후 수령.
- 에이전트: 봇에게 우편을 보내고 `check_mail` → `claim_mail`로 회수되는지
  (되지 않으면 동등성 위반이다).

**성능** — 캐릭터당 최대 30행. 5,000명 전원 만석이어도 150,000행으로 SQLite에 무해하다.
런타임 메모리에는 **읽지 않은 개수(u16)만** 상주시키고 본문은 패널을 열 때 로드한다.

---

### IMP-2.2. 세이브 포인트 + 리스폰

| | |
|---|---|
| 결정 | 채택 (09 #4-a) |
| 난이도 | 소~중 |
| 선행 조건 | 없음 |
| 프로토콜 변경 | 있음 — `ClientMessage::SetSavePoint { npc_player_id }`, `ServerMessage::SavePointSet { position }` |
| 저장 스키마 변경 | 있음 — `characters`에 `save_x/save_y/save_z/save_rotation` (NULL 허용) |

**손댈 파일**
- `server/src/auth.rs` — `ensure_character_save_point_columns`(`ALTER TABLE ... ADD COLUMN`,
  `ensure_character_item_columns`(:537)와 같은 패턴), `CHARACTER_COLUMNS`(:165),
  `character_record_from_row`(:167), `CharacterSaveData`(:149),
  `write_character_states`(:268).
- `server/src/game_state/player.rs:1563` `respawn_player` — 세이브 포인트 우선.
- `server/src/game_state/inventory.rs:885` `use_return_scroll` — 동일 목적지.
- `server/src/game_state/mod.rs` — `save_points: Arc<RwLock<HashMap<PlayerId, Position>>>`
  및 `register_player_character` / `unregister_player_character`에서 등록·해제.
- `server/src/connection.rs` — `EnterGame` arm에서 레코드의 세이브 포인트를 메모리에 적재.

**구현 방향**
좌표를 클라이언트가 보내게 두지 않는다. `SetSavePoint`는 **NPC id만** 싣고 서버가
그 NPC의 현재 위치를 저장한다 — 임의 좌표 세이브는 던전 앞 세이브를 허용해
IMP-2.4의 "던전 워프 불가"를 우회한다. 검증 3종: NPC와의 거리(`MAX_TRADE_DISTANCE`
6.0m 재사용), `floor_level == 0`, 그리고 `dungeon::entrance_at`의 footprint 밖.
`NULL = 월드 스폰`으로 두는 것이 마이그레이션의 핵심이다 — 컬럼을 추가해도 기존
캐릭터는 `respawn_player`가 지금과 똑같이 `world_config().spawn_position`을 쓴다.
`use_return_scroll`도 같은 목적지로 바꾼다. 그러지 않으면 "귀환 주문서는 수도로,
사망은 세이브 포인트로"라는 두 규칙을 플레이어가 따로 외워야 하고, 도시 서비스
NPC를 찾아갈 이유가 반감된다.
사망 시 층은 지금처럼 항상 0으로 강제한다(던전 깊이·낡은 하우징 층 정리).

**데이터 스키마**

| 컬럼 | 타입 | 기본 | 의미 |
|------|------|------|------|
| `save_x` / `save_y` / `save_z` | REAL NULL | NULL | 지정된 리스폰 좌표 |
| `save_rotation` | REAL NULL | NULL | 리스폰 시 바라볼 방향 |

**마이그레이션** — `ALTER TABLE`로 NULL 컬럼 4개 추가. 기존 캐릭터 동작 무변화.
롤백 시 컬럼이 남지만 구버전 서버는 `CHARACTER_COLUMNS`에 없으므로 무시한다.

**검증**
- `auth.rs` 테스트: 세이브 포인트 저장/로드 왕복, NULL 캐릭터의 기본 동작.
- `game_state` 테스트: 던전 안/공중층에서의 `SetSavePoint` 거부, 거리 초과 거부.
- 인게임: 도시에서 세이브 → 멀리서 사망 → 도시에서 부활, 귀환 주문서도 같은 지점.

**성능** — 캐릭터당 f32 4개. 배치 세이브에 컬럼 4개가 붙을 뿐 새 쓰기 경로 없음.

---

### IMP-2.3. 창고 (Storage)

| | |
|---|---|
| 결정 | 채택 (09 #6, #4-b) |
| 난이도 | 대 |
| 선행 조건 | IMP-2.2(같은 NPC에 묶음), SPK-2(델타 측정), IMP-0.1(슬롯 상한 확정) |
| 프로토콜 변경 | 있음 — `OpenStorage` / `StorageDeposit` / `StorageWithdraw` / `CloseStorage`, `StorageOpened` / `StorageSlotChanged` |
| 저장 스키마 변경 | 있음 — `character_storage` 테이블 |

**손댈 파일**
- `server/src/auth.rs` — `ensure_storage_schema` + `AuthService::new`(:371) 블록에 등록,
  `load_storage` / `save_batch`(:1266)에 창고 쓰기 추가(기존 `character_items`
  전량 교체 패턴(:322~:324)을 그대로 복제).
- `server/src/game_state/mod.rs` — `storages: Arc<RwLock<HashMap<PlayerId, Vec<Option<ItemInstance>>>>>`,
  `dirty_storages: Arc<RwLock<HashSet<PlayerId>>>`.
- `server/src/game_state/storage.rs`(신규) — `impl super::GameState`.
- `server/src/game_state/player.rs:644` `flush_dirty_saves` / `:577`
  `persist_shutdown_snapshot` / `:605` `collect_shutdown_snapshot` — 창고 합류.
- `server/src/game_state/player.rs:302` `unregister_player_character` — 창고 해제(누수 방지).
- `client/src/lib/stores/storageStore.ts`, `StoragePanel.svelte`, `overlayStack.ts`(layer 1),
  `messageHandlers.ts`.
- `agent-client/src/driver/action.rs` — `deposit` / `withdraw` 액션 + `ACTION_SPECS`.

**구현 방향**
`STORAGE_SLOTS = 120`으로 시작한다. RO의 600은 5,000명 × 600행 = 300만 행이고,
`character_items`가 세이브마다 **전량 삭제 후 재삽입**되는 현재 패턴을 그대로 쓰면
플러시 비용이 폭발한다. 120이면 최대 60만 행이고, 실제로는 **열려 있는 동안에만
메모리에 상주**시키므로 런타임 상주분은 동시 창고 이용자 수에 비례한다.
창고는 **무게가 없다**. 무게 제한이 강한 게임의 짝이라는 것이 09 #6의 근거이므로
창고에까지 무게를 걸면 도입 의미가 없다. 대신 슬롯 수가 유일한 상한이다.
원자성은 `buy_item`(`server/src/game_state/trading.rs:558`)의 패턴을 그대로 쓴다 —
`inventories` 쓰기 락과 `storages` 쓰기 락을 **함께 쥔 채로** 수량 재확인·차감·삽입을
끝내고, 모든 조기 반환 경로에서 둘 다 놓는다. 이것이 아이템 복제 방지의 전부다.
전송은 델타다: 열 때 `StorageOpened`로 한 번, 이후 이동마다 `StorageSlotChanged`
단건 2개(출발 슬롯·도착 슬롯). 스냅샷을 다시 밀면 120슬롯 × 이동 횟수가 된다.
접근은 NPC 근처에서만 — 열려 있는 상태에서 멀어지면 서버가 세션을 닫는다
(`tick_shop_holds`(`trading.rs:403`)와 같은 방식으로 기존 8초 틱에 얹는다).

**데이터 스키마**

```sql
CREATE TABLE IF NOT EXISTS character_storage (
  character_id INTEGER NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
  slot_index   INTEGER NOT NULL,
  item_def_id  TEXT NOT NULL,
  quantity     INTEGER NOT NULL,
  enchant      INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (character_id, slot_index)
);
```

**마이그레이션** — 신규 테이블. 기존 캐릭터는 빈 창고.
`character_items`의 스키마 변형 이력(`ensure_character_item_columns`, :537)을 참고해
처음부터 `enchant`를 넣는다.

**검증**
- `auth.rs` 테스트: 왕복 저장, 슬롯 인덱스 충돌, 캐스케이드 삭제.
- `game_state` 테스트: **동시 입출금 복제 시도**(같은 instance_id로 두 번 입금),
  슬롯 초과 거부, NPC 이탈 시 세션 종료, 로그아웃 후 재접속 시 내용 보존.
- 인게임: 무게 한계까지 채운 뒤 창고에 넣고 다시 꺼내기, 서버 재시작 후 잔존.

**성능** — 제약 (c)의 정면 대상. 세 가지 가드레일: ① 슬롯 120 고정,
② **열려 있는 동안에만 메모리 상주**, ③ `flush_dirty_saves`의 기존 배치에 합류
(새 디스크 경로 없음). 5,000명이 동시에 창고를 여는 상황은 없다고 가정하되,
`storages` 맵 크기를 메트릭으로 남긴다.

---

### IMP-2.4. 유료 이동 + "던전은 워프 불가"

| | |
|---|---|
| 결정 | 채택 (09 #4-c, #5) |
| 난이도 | 중 |
| 선행 조건 | IMP-2.2, SPK-3(로딩 폭풍 측정) |
| 프로토콜 변경 | 있음 — `ClientMessage::RequestTravel { npc_player_id, node_id }`, `ServerMessage::TravelDestinations { nodes }` / `TravelDenied { reason }` |
| 저장 스키마 변경 | 없음 |

**손댈 파일**
- `data-src/travel_nodes.csv`(신규) — 목적지 테이블.
- `server/src/travel_defs.rs`(신규) — `debuff_defs.rs`와 동일한 정의 로드 패턴,
  `include_str!("../../data/travel_nodes.json")`.
- `server/src/main.rs` — `mod travel_defs;` + 부팅 시 좌표가 던전 footprint 밖인지 assert.
- `server/src/game_state/travel.rs`(신규) — 검증 후 `teleport_player`(`player.rs`) 호출.
- `server/src/connection.rs:733` — arm 2개.
- `client/src/lib/data/travelDefs.ts` + `TravelPanel.svelte`.
- `agent-client/src/driver/action.rs` — `travel` 액션.

**구현 방향**
목적지는 **CSV의 고정 노드만**이다. 임의 좌표 텔레포트를 허용하는 순간 32km 월드의
이동이 콘텐츠에서 UI로 격하된다(09 #5의 근거).
"던전 워프 불가"는 두 방향 모두에서 강제한다. ① 목적지: `travel_nodes.csv`에 던전
좌표를 넣지 못하도록 **부팅 시** 모든 노드를 `dungeon::entrance_at`으로 검사해
footprint 안이면 기동 실패시킨다 — 런타임 검사보다 데이터 실수를 잡는 데 확실하다.
② 출발지: `floor_level != 0`이면 거부한다. 던전 깊이는 음수 층이므로 이 한 줄이
"던전 안에서 도시로 튀기"를 막는다.
추가 게이트: 서비스 NPC와의 거리, `Self::in_combat`(전투 중 탈출 금지 — 귀환 주문서와
같은 규칙), 소지금. **요금은 소각한다** — NPC 지갑에 넣으면 싱크가 아니라 이전이다
(`player_gold` 차감만, 어느 지갑에도 입금하지 않음).
도착 처리는 새로 만들지 않고 `use_return_scroll`이 이미 쓰는 `teleport_player`를
그대로 재사용한다. 로딩 폭풍(09 #4 성능 주의)에 대한 지금의 답은 **목적지 수를
10 미만으로 제한**하는 것이다 — 소수 지점에 도착이 몰려야 클라이언트 타일 캐시와
`RegionImageCache`가 실제로 재사용된다. 추가 예열이 필요해지면
[doc/LOADING_OPTIMIZATION.md](../LOADING_OPTIMIZATION.md)의 프리페치 경로를 붙인다.

**데이터 스키마**

| 컬럼 | 의미 |
|------|------|
| `id` | 노드 id |
| `name` | 표시 이름 |
| `x` / `y` / `z` | 도착 좌표 (반드시 지상, 던전 footprint 밖) |
| `fare` | 요금(구리, 소각) |
| `minLevel` | 이용 최소 레벨 (빈 칸 = 1) |

```csv
id,name,x,y,z,fare,minLevel
capital,Capital,-1475.2,0.7,4741.6,0,1
crypt_town,Crypt Town,-1402.0,1.1,4680.0,600,3
```

**마이그레이션** — 없음.

**검증**
- 부팅 테스트: 던전 footprint 안 좌표를 가진 노드가 있으면 기동 실패.
- `game_state` 테스트: 음수 층에서 요청 거부, 전투 중 거부, 소지금 부족 거부,
  요금이 어느 지갑에도 들어가지 않는지(총 통화량 감소 확인).
- 인게임: 던전 3층에서 이동 시도 → 거부 문구, 지상 귀환 후 이동 성공, 도착 지점의
  프레임 드랍 관찰.

**성능** — 이동 자체는 기존 텔레포트 1회. 진짜 비용은 클라이언트 스트리밍이므로
가드레일은 **목적지 수 상한**이다. 노드를 늘릴 때마다 도착 지점 로딩을 측정한다.

---

### IMP-2.5. 헌팅 보드 반복 퀘스트

| | |
|---|---|
| 결정 | 채택 (09 #2) |
| 난이도 | 대 |
| 선행 조건 | IMP-2.1(보상 지급 경로), IMP-1.1(레벨 구간 설계의 전제) |
| 프로토콜 변경 | 있음 — `OpenQuestBoard` / `AcceptQuest` / `AbandonQuest` / `TurnInQuest`, `QuestBoard` / `QuestAccepted` / `QuestProgress` / `QuestCompleted` + `QuestOffer` 페이로드 (v31 → **v32**) |
| 저장 스키마 변경 | 있음 — `character_quests` 테이블 |

**손댈 파일**
- `data-src/hunting_quests.csv`(신규).
- `server/src/quest_defs.rs`(신규) — 부팅 시 `monsterId`가 `monsters.csv`에 있는지,
  `rewardItem`이 `items.csv`에 있는지 assert(`world_drop_defs.rs`의 교차 검증 패턴).
- `server/src/auth.rs` — `ensure_quest_schema`, 로드/저장.
- `server/src/game_state/mod.rs` — `quest_progress`, `dirty_quests`.
- `server/src/game_state/quest.rs`(신규).
- `server/src/game_state/combat.rs:499~:511` — **처치 카운터 훅**. XP 수령자 목록과
  동일한 리스트를 사용한다.
- `server/src/game_state/player.rs:644` `flush_dirty_saves` 합류, `:302` 정리.
- `client/src/lib/stores/questStore.ts`, `QuestBoardPanel.svelte`, `QuestTracker.svelte`,
  `overlayStack.ts`.
- `agent-client/` — `AgentAction::QuestBoard`/`AcceptQuest`/`TurnInQuest`.
  **`quest_board`가 추가된 이유**: 봇은 읽을 수 없는 계약을 수락할 수 없다. 진행 상황은
  `format_event`가 `[Quest] id 3/10` 형태로 흘려주므로 별도 월드 상태 필드는 두지 않았다.
- `server/src/game_state/combat.rs` — 처치 훅 외에 `bank_xp`를 추출했다.
  계약 보상 XP는 레벨 차 감쇠(IMP-1.1)를 **받지 않는 고정액**이라, 감쇠를 품고 있던
  `grant_monster_kill_xp`의 본체를 공용 함수로 빼고 두 호출자가 각자의 금액 규칙을
  넘긴다.
- `client/src/lib/components/GameHud.svelte` — 보드를 여는 버튼. 도시 서비스 NPC가
  아직 없으므로(IMP-2.2) **HUD 버튼이 임시 진입점**이고, 열리는 보드는 `capital`
  하나로 고정한다. NPC가 생기면 이 버튼은 상호작용으로 대체된다.

**구현 방향**
카운터 훅은 이미 정확히 한 곳에 있다: `combat.rs`가 `party_members_sharing_kill`로
수령자 목록을 만들고(:501) XP를 나눠 주는 자리(:511). **퀘스트 카운트도 같은 목록에
준다** — XP를 나눠 받는 파티원이 사냥 진척은 못 받는다면 파티 플레이가 벌점이 된다.
메모리는 제약 (c)를 정면으로 맞는다. `HashMap<PlayerId, HashMap<String, u32>>`는
5,000명 × 문자열 키로 커지므로, **퀘스트 id를 로드 시 `u16`으로 인턴**하고
`HashMap<PlayerId, Vec<(u16, u16)>>`(수락 상한 5개)로 둔다. 처치당 비용은 최대
5회 비교이고 할당이 없다.
진행 푸시는 **변화가 있을 때만, 해당 플레이어에게 직접** 보낸다 — 브로드캐스트 금지.
보상은 IMP-2.1의 우편으로 지급한다. 이것이 12 문서가 우편을 퀘스트보다 먼저 넣으라고
한 이유이고, 인벤토리 만석 시 보상 증발이라는 전형적인 버그 클래스를 통째로 없앤다.
UI는 `overlayStack.ts:24`의 `OVERLAYS`에 `questBoard`(layer 0)로 등록하고,
HUD 상단의 트래커는 오버레이가 아니라 항상 표시되는 위젯으로 둔다.

**데이터 스키마**

| 컬럼 | 의미 |
|------|------|
| `id` | 퀘스트 id (u16 인턴 대상) |
| `boardId` | 어느 도시 보드에 걸리는가 |
| `name` | 표시 이름 |
| `monsterId` | 대상 몬스터 (`monsters.csv`) |
| `count` | 필요 처치 수 |
| `minLevel` / `maxLevel` | 수락 가능 레벨 구간 (IMP-1.1 감쇠와 짝) |
| `rewardXp` / `rewardZeny` | 보상 |
| `rewardItem` | 보상 아이템 id (빈 칸 = 없음) |
| `dailyLimit` | 하루 완료 상한 (0 = 무제한, IMP-2.6) |

```csv
id,boardId,name,monsterId,count,minLevel,maxLevel,rewardXp,rewardZeny,rewardItem,dailyLimit
hb_kobold_10,capital,Kobold Cull,kobold,10,1,4,120,300,,0
hb_troll_daily,capital,Troll Contract,troll,5,8,99,4000,6000,healing_potion,1
```

```sql
CREATE TABLE IF NOT EXISTS character_quests (
  character_id INTEGER NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
  quest_id     TEXT NOT NULL,
  progress     INTEGER NOT NULL DEFAULT 0,
  day_key      INTEGER NOT NULL DEFAULT 0,   -- IMP-2.6
  day_count    INTEGER NOT NULL DEFAULT 0,   -- IMP-2.6
  PRIMARY KEY (character_id, quest_id)
);
```

**마이그레이션** — 신규 테이블. 기존 캐릭터는 수락한 퀘스트 없음.

**포기(abandon)의 규칙** — 진척은 0으로 돌아가지만 **그날 쓴 일일 횟수는 돌려주지
않는다.** `character_quests` 행을 지우지 않고 `progress`만 0으로 만드는 이유가 이것이다.
지웠다면 "일일 계약 완료 → 재수락 → 포기"로 한도를 세탁할 수 있다.

**검증** (구현 완료 — 12개 테스트)
- `server/src/quest_defs.rs`: 정의 교차검증 2개(부팅 assert, 인턴 왕복).
- `server/src/game_state/tests/quest_tests.rs`: 파티 전원 카운트, 대상 외 몬스터는
  무반응, 레벨 구간 밖 수락 거부, 수락 상한, 반납 시 **우편으로** 보상 + 일일 소진,
  미완료 반납 거부, 포기 시 진척만 초기화, 저장·재로드 왕복,
  `utc_day_key` 경계(자정 전후·음수 시각), 날짜가 바뀌면 일일 카운트 리셋.
- 인게임: 보드에서 수락 → 대상 사냥 → 트래커 증가 → 반납 → 우편 도착 → 수령.

**성능** — 처치당 최대 5회 비교, 할당 0. 저장은 배치 합류.
5,000명 × 수락 5 = 25,000 엔트리(`(u16,u16)` = 4바이트) → 100KB 수준.
**틱에서의 전체 순회는 만들지 않는다** — 진행은 전적으로 이벤트 구동이다.

---

### IMP-2.6. 일일 한도 + 고효율

| | |
|---|---|
| 결정 | 채택 (09 #3) |
| 난이도 | 소 (단 IMP-2.5와 **같은 PR**이어야 한다) |
| 선행 조건 | IMP-2.5, IMP-0.1(한도 수치 확정) |
| 프로토콜 변경 | 없음 (`QuestBoard`에 남은 횟수 필드가 이미 포함) |
| 저장 스키마 변경 | 없음 (`character_quests.day_key` / `day_count`가 IMP-2.5에 포함) |

**손댈 파일**
- `server/src/game_state/quest.rs` — `day_key` 갱신 판정 한 곳.
- `data-src/hunting_quests.csv` — `dailyLimit` 열 채우기(밸런스 데이터).
- `doc/` 신규 퀘스트 문서 — 리셋 시각 명시.

**구현 방향**
**리셋 기준은 게임 시계가 아니라 실시간 UTC 날짜다.** 이 프로젝트의 게임 하루는
실시간 3시간이다(`server/src/game_state/time.rs:3`, `REAL_DAY_DURATION_SECONDS =
3.0 * 60.0 * 60.0`). 게임 자정을 리셋으로 쓰면 "일일 한도"가 3시간마다 풀려
09 #3이 막으려던 무한 파밍이 그대로 남는다. `day_key = unix_secs / 86400`
(UTC 자정 기준)으로 둔다.
판정은 **지연(lazy) 방식** 한 곳에서만 한다 — 반납 시점에 `day_key`를 현재 값과
비교해 다르면 `day_count = 0`으로 리셋하고 진행한다. 로그인 시 전체 순회나
자정 스윕 틱을 만들지 않는다(제약 (c) 체크리스트 1번).
"고효율"은 코드가 아니라 데이터다: `dailyLimit > 0`인 계약의 `rewardXp`를 무제한
반복 퀘스트의 3~5배로 잡는다. 이 배율을 1일차부터 넣는 것이 09 §4의 3번 기준이며,
나중에 올리면 밸런스 조정이지만 나중에 **한도를 새로 거는 것은 너프로 읽힌다**.

**데이터 스키마** — IMP-2.5의 `dailyLimit` 컬럼과 `character_quests`의 두 컬럼이 전부.

**마이그레이션** — 없음(IMP-2.5와 동시 도입).

**검증**
- 단위 테스트: `day_key` 경계(UTC 자정 직전/직후), 한도 소진 후 반납 거부,
  날짜가 바뀌면 자동 리셋.
- 인게임: 일일 계약 완료 → 재수락 시 남은 횟수 0 표시 → UTC 자정 이후 복구.
  (테스트 편의를 위해 `--debug-day-offset` 같은 관리자 플래그를 고려 — `requires_admin`
  (`server/src/connection.rs:646`)에 반드시 등록할 것.)

**성능** — 반납당 정수 비교 2회. 새 틱 없음.

---

### IMP-2.7. 미니보스 (장주기 + 변량 리스폰)

| | |
|---|---|
| 결정 | 채택 (09 #9) |
| 난이도 | 중 |
| 선행 조건 | IMP-1.2(`boss` 플래그) |
| 프로토콜 변경 | 없음 (기존 `MonsterSpawned` 재사용) |
| 저장 스키마 변경 | 없음 (리스폰 타이머는 메모리, 재시작 시 즉시 스폰 대기) |

**손댈 파일**
- `data-src/world_bosses.csv`(신규).
- `server/src/world_boss_defs.rs`(신규).
- `server/src/game_state/monster.rs` — `tick_world_bosses`(신규) 또는
  `tick_monster_spawns`(:854)와 별개 틱.
- `server/src/main.rs:84` — `run_ticks("world_bosses", Duration::from_secs(30), ...)`.
- `client/src/lib/components/map-editor/MapEditorCursor.svelte` — 커서 좌표 복사 버튼(운영 편의).

**구현 방향**
**doc 10 §2.3의 전제를 먼저 정정한다.** "존 스폰 JSON에 `respawnVarianceSecs` 추가"는
서버가 `data/terrain/zones/*.json`의 `monsterSpawns`를 읽는다고 가정하지만,
**서버는 그 배열을 읽지 않는다** — `load_no_spawn_zones_from_regions`
(`server/src/world_config.rs:93`)는 `noSpawnZones`만 읽고, 지상 스폰은
`data-src/world.json`의 `ambientSpawns`를 근거로 소유자 클라이언트에게 위치를
요청하는 모델이다(`tick_monster_spawns`, `monster.rs:854`). `monsterSpawns`를
읽고 쓰는 유일한 코드는 맵 에디터다(`client/src/lib/managers/zoneManager.ts:24`).
따라서 존 기반 정원 스포너를 서버에 새로 만들거나(대), 미니보스만을 위한 전용
데이터를 두는(중) 두 갈래인데, **후자를 택한다**. 미니보스는 정원이 아니라
"고정 위치 + 장주기 + 변량"이고 사각형이 필요 없다. 존 스포너 전면 도입은 별도
과제로 남긴다(그때 `monsterSpawns`가 이미 에디터에 있으므로 데이터는 재사용된다).
스폰 조건은 세 가지: 타이머 만료, 좌표 근처에 소유자가 될 플레이어 존재
(`players_within_position`, `player.rs:2098`), 그리고 아직 살아 있지 않을 것.
근처에 아무도 없으면 **스폰을 미룬다** — 새 `MonsterLifecycle` 변형을 만들지 않기
위해서다. `Ambient`로 두면 무주공산일 때 despawn되지만, 리스폰 타이머가 서버에
남아 있으므로 다음 플레이어가 오면 다시 나타난다.
소유자별 스폰 캡(`maxMonstersPerPlayer` 30)은 **우회하지 않는다** — 미니보스도
캡을 소비한다(09 #9의 성능 주의).
운영 도구 문제(doc 10의 지적)에 대한 답: 좌표는 맵 에디터 커서에서 복사하고 CSV에
붙인다. 존 편집 UI에 필드를 추가하는 대신 커서 좌표 복사 버튼 하나로 끝난다.

**데이터 스키마**

| 컬럼 | 의미 |
|------|------|
| `id` | 스폰 지점 id |
| `monsterId` | `monsters.csv`의 몬스터 (부팅 시 `boss=true` assert) |
| `x` / `y` / `z` | 고정 스폰 좌표 |
| `respawnBaseSecs` | 처치 후 기본 대기 (예 1800) |
| `respawnVarianceSecs` | 여기에 0..N 균등 난수를 더한다 (예 600) |

```csv
id,monsterId,x,y,z,respawnBaseSecs,respawnVarianceSecs
crypt_ogre,ogre_boss,-1390.0,1.2,4705.0,1800,600
```

**마이그레이션** — 없음.

**검증**
- 부팅 테스트: `monsterId`가 없거나 `boss != true`면 기동 실패.
- `game_state` 테스트: 처치 후 base 이내 재스폰 없음, base+variance 이후 스폰,
  근처 플레이어 0명이면 대기, 소유자 캡을 소비하는지.
- 인게임: 미니보스 처치 후 시각 기록 → 변량 범위 안에서 재등장.

**성능** — 30초 틱에서 CSV 행 수만큼 순회(현재 한 자릿수). 플레이어 전체 순회가 아니라
`players_within_position`의 공간 해시 조회이므로 O(cells)다.
행 수가 수십을 넘으면 그때 타이머 힙으로 바꾼다.

---

### IMP-2.8. MVP 기여도 보너스

| | |
|---|---|
| 결정 | 채택 (09 #10) |
| 난이도 | 중 |
| 선행 조건 | IMP-1.2(`boss` 플래그), IMP-2.7(찾아다닐 대상) |
| 프로토콜 변경 | 있음 — `ServerMessage::MvpBonus { xp, item_def_id }` (수령자 직행) |
| 저장 스키마 변경 | 없음 |

**손댈 파일**
- `server/src/game_state/mod.rs` — `boss_damage: Arc<RwLock<HashMap<String, Vec<(PlayerId, u32)>>>>`.
- `server/src/game_state/combat.rs` — 데미지 적용 구간(:411~:434의 `monsters` 쓰기 가드)에서
  누적, 사망 처리(:494 부근)에서 최대 기여자 산정, `despawn_monsters` 경로에서 제거.
- `shared/src/messages.rs:577` + `PROTOCOL_VERSION`.
- `client/src/lib/network/messageHandlers.ts` — 토스트.
- `agent-client/src/driver/prompt.rs:164` — `[Mvp]` 라인.

**구현 방향**
기록은 **보스에만** 한다(`MonsterDefinition::is_boss()`). 일반 몹까지 기록하면
`MonsterRegistry`가 겨냥하는 십만 단위 몬스터에 누적 벡터가 붙는다.
메모리 상한이 필수다: 몬스터당 최대 16 기여자, 초과 시 최소 기여자를 교체한다
(정확도보다 상한이 중요하다 — 상위 1명만 쓰기 때문이다). 벡터는 몬스터 사망·despawn·
`remove_monsters_by_owner` 세 경로 모두에서 제거해야 한다.
**락 안에서 할당하지 않는다**(제약 (c) 체크리스트 3번). 데미지 적용은 `monsters`
쓰기 가드 안에서 일어나므로, `boss_damage`는 별도 락이고 엔트리 벡터는
`Vec::with_capacity(16)`으로 스폰 시 미리 잡아 둔다.
보상은 **파티 분배와 완전히 분리된 두 번째 지급**이다. `grant_monster_kill_xp`를 한 번
더 호출하되 수령자는 1명이고, 아이템 보너스는 `dungeons.csv`의 `chestDrops`와 무관한
별도 롤이다. "막타가 아니라 기여"라는 09 #10의 요지는 여기서 나온다 — 최종 타격자와
최대 기여자가 다를 수 있고, 그것이 정상 동작이다.

**데이터 스키마** — 없음. 보너스 배율은 `server/src/game_state/combat.rs`의
`pub const MVP_XP_BONUS_PCT: u32 = 50`.

**마이그레이션** — 없음.

**검증**
- `game_state` 테스트: 최대 기여자 ≠ 막타인 경우의 지급 대상, 17명 이상 기여 시
  벡터 길이 16 유지, despawn 후 맵이 비는지.
- 인게임: 2인 파티로 보스를 잡되 한쪽이 대부분의 피해를 넣고, 보너스가 그쪽으로
  가는지.

**성능** — 보스 피격당 벡터 선형 탐색(≤16). 보스는 월드에 한 자릿수 개체.
일반 몹은 이 경로에 진입조차 하지 않는다.

---

# Phase 3 — 새 성장 축

| ID | 항목 | 09 # | 10 § |
|----|------|------|------|
| IMP-3.1 | 시간 4분할 (VCT/FCT/딜레이/쿨다운) | 12 | 3.1 선결 |
| IMP-3.2 | 전투 스킬 시스템 | 40 (09 #12 선결, #28은 기각·아이디어만 차용) | 3.1 |
| IMP-3.3 | 방어 2단 구조 | 13 | 3.2 |
| IMP-3.4 | 경제 스킬 `Trading` | 15 | 3.3 |
| IMP-3.5 | 고액 거래 수수료 | 14 | 3.4 |
| IMP-3.6 | 코스튬 레이어 | 18 | — |
| IMP-3.7 | 업적 / 칭호 | 38 (12 §5) | — |

---

### IMP-3.1. 시간 4분할

| | |
|---|---|
| 결정 | 채택(스킬 도입 시 선결) (09 #12) |
| 난이도 | 중 |
| 선행 조건 | 없음 — **IMP-3.2보다 먼저 확정** |
| 프로토콜 변경 | 없음 (타입과 상수만 확정. 메시지는 IMP-3.2에서) |
| 저장 스키마 변경 | 없음 |

**손댈 파일**
- `shared/src/cast.rs`(신규) — `CastTiming`, `resolve_cast_ms`.
- `shared/src/lib.rs` — `pub mod cast;` + 플랫 재수출.
- `shared/src/wasm_api.rs` — `cast_timing_for(...)` 게터(클라이언트가 캐스팅 바를
  같은 숫자로 그리게).
- `server/src/game_state/mod.rs` — `casting: HashMap<PlayerId, CastState>`,
  `global_cast_delay_until: HashMap<PlayerId, u64>`, `skill_cooldowns`.
- `doc/COMBAT.md` — 4분할 정의와 각 시간대에 허용되는 행동 표.

**구현 방향**
이 항목은 **사용자에게 보이는 것이 없다.** 그럼에도 별도 항목인 이유는 09 §4의 3번
기준 — 스킬을 넣은 뒤에 넣으면 모든 스킬 정의와 애니메이션 타이밍을 다시 잡아야
한다. 확정해야 할 것은 네 시간의 **의미와 허용 행동**이다:

| 시간 | 중 허용 | 저장 위치 |
|------|---------|-----------|
| VCT (가변 시전) | 이동 시 캔슬 | `CastState` |
| FCT (고정 시전) | 캔슬 불가 | `CastState` |
| after-cast delay | 이동·평타 O, 모든 스킬 X | `global_cast_delay_until` |
| cooldown | 이동·평타·다른 스킬 O | `skill_cooldowns[(player, skill)]` |

VCT 단축식은 제약 (a) 때문에 RO의 `√[(DEX×2 + INT) ÷ 530]`을 쓸 수 없다. 3~18에서
`√`는 구간을 뭉갠다. 대신 어빌리티 모디파이어 선형 + 상한으로 임계점만 남긴다:

```
vct_ms = base_vct_ms × (100 − clamp((dex_mod × 2 + int_mod) × 4, 0, 60)) / 100
```

DEX 18 / INT 18(mod +4 둘)이면 `(8+4)×4 = 48%` 단축, 상한 60%에서 무캐스팅에는
못 미친다 — "임계점은 있되 완전 무캐스팅은 장비까지 맞춘 뒤"라는 구조를 유지한다.
평타 주기는 그대로 애니메이션에 묶어 둔다 — `claim_player_attack_window`
(`server/src/game_state/combat.rs:125`)와 `data-src/player_anim_timing.csv`가 이미
그 역할이고, ASPD 공식은 09 #29에서 기각했다. after-cast delay는 평타를 막지 않으므로
이 두 시스템은 서로 간섭하지 않는다.

**데이터 스키마** — 없음. IMP-3.2의 `skills.csv`가 이 네 값을 컬럼으로 갖는다.

**마이그레이션** — 없음.

**검증**
- `shared/src/cast.rs` 테스트: 단축 상한 60%, 최저 능력치에서 단축 0%,
  네 시간의 겹침 규칙(after-cast delay와 cooldown이 동시에 시작).
- 수동 검증은 IMP-3.2와 함께.

**성능** — 플레이어당 `Option<CastState>` + 스킬별 만료 시각. **틱을 만들지 않는다** —
만료는 "다음 사용 시도 시점에 현재 시각과 비교"하는 지연 판정이다. 5,000명 × 만료
스윕 틱을 도입하는 순간 체크리스트 1번을 위반한다.

---

### IMP-3.2. 전투 스킬 시스템

| | |
|---|---|
| 결정 | 채택 (10 §3.1; 09 #28의 "상한 해제" 아이디어만 차용) |
| 난이도 | 대 (이 문서 최대 항목) |
| 선행 조건 | **IMP-3.1** |
| 프로토콜 변경 | 있음 — `UseSkill` / `CancelCast`, `SkillCastStarted` / `SkillResult` / `SkillCooldowns` / `SkillPointsUpdate` / `SkillLearned` |
| 저장 스키마 변경 | 있음 — `characters`에 `job_xp` / `skill_points`, `character_skills` 재사용 |

**손댈 파일**
- `data-src/skills.csv`(신규).
- `shared/src/skills.rs:13` `SkillId` — 전투 스킬 변형 추가 + `as_str`/`display_name`/`FromStr`.
- `shared/src/messages.rs` — 메시지 7종.
- `server/src/skill_defs.rs`(신규), `server/src/game_state/skill.rs`(신규).
- `server/src/auth.rs` — `job_xp`/`skill_points` 컬럼(`CHARACTER_COLUMNS`:165,
  `character_record_from_row`:167, `CharacterSaveData`:149, `write_character_states`:268),
  기존 `character_skills` 테이블(:614)에 배운 스킬 레벨 저장.
- `server/src/game_state/combat.rs:499` — 처치 시 `job_xp`도 지급.
- `data-src/player_anim_timing.csv` — 스킬 클립 타이밍.
- `client/` — `SkillBar.svelte`, `SkillTreePanel.svelte`, 캐스팅 바, `messageHandlers.ts`.
- `agent-client/src/driver/action.rs` — `AgentAction::UseSkill` + `ACTION_SPECS` 항목.

**구현 방향**
성장 축은 **Job Level이 아니라 스킬 포인트**로 좁힌다(09 #28은 전직 트리를 기각했다).
`job_xp`는 base XP와 **같은 처치 이벤트에서 다른 곡선으로** 쌓이고, 임계마다
`skill_points += 1`을 준다. 곡선은 `shared/src/skills.rs`의 기존
`skill_xp_for_level`(100·n(n+1)(2n+1)/6)을 재사용해 새 곡선을 만들지 않는다.
"상한 해제형 성장"(09 #28)은 `requiresSkill` / `requiresSkillLevel` 두 컬럼으로
표현한다 — 선행 스킬을 일정 레벨까지 올려야 다음 스킬이 열린다.
**서버가 쿨다운·사거리·자원·시전을 전부 판정하고 클라이언트 예측은 금지한다.**
클라이언트가 하는 일은 캐스팅 바 렌더와 입력 전송뿐이다. 이것이 제약 (c)의 요구다 —
에이전트는 예측 코드를 갖지 않으므로, 예측이 게임플레이의 일부가 되면 사람만 유리해진다.
`SkillId`를 확장하는 이유(새 enum을 만들지 않는 이유)는 `character_skills` 테이블과
`Skills::add_xp`, 그리고 `SkillsUpdate` 메시지가 이미 스킬 비의존적으로 작성되어
있기 때문이다 — 낚시가 그 그릇이라는 01 문서의 지적이 코드 수준에서 맞다.
피해 계산은 `server/src/game/combat.rs`의 `roll_attack_with_extra_damage_roll`(:98)을
재사용한다(스킬은 `extra_damage_roll`을 채우는 형태). 새 데미지 파이프라인을 만들지
않는 것이 IMP-3.3과의 충돌을 피하는 길이다.

**데이터 스키마**

| 컬럼 | 의미 |
|------|------|
| `id` | `SkillId`의 serde 이름과 일치 |
| `name` | 표시 이름 |
| `maxLevel` | 1~10 |
| `vctMs` / `fctMs` / `afterCastDelayMs` / `cooldownMs` | IMP-3.1의 네 시간 |
| `range` | 사거리 m (서버 검증) |
| `target` | `self` / `enemy` / `ally` / `ground` |
| `costSatiation` | 자원 소모 (마나가 없으므로 허기를 쓴다 — 09 #31이 몬스터 SP를 기각한 것과 같은 이유로 플레이어에게도 새 자원 축을 만들지 않는다) |
| `damageDice` | 다이스 문자열 (`damageRoll`과 동일 문법) |
| `damageBonusStat` | `str`/`dex`/`int`/`wis` |
| `animClip` | 애니메이션 클립 이름 |
| `requiresSkill` / `requiresSkillLevel` | 해금 사다리 |

```csv
id,name,maxLevel,vctMs,fctMs,afterCastDelayMs,cooldownMs,range,target,costSatiation,damageDice,damageBonusStat,animClip,requiresSkill,requiresSkillLevel
power_strike,Power Strike,5,0,300,500,4000,2.0,enemy,15,2d6,str,attack_heavy,,
cleave,Cleave,5,400,200,800,9000,2.5,enemy,25,3d6,str,attack_sweep,power_strike,3
```

**마이그레이션** — `characters`에 `job_xp INTEGER NOT NULL DEFAULT 0`,
`skill_points INTEGER NOT NULL DEFAULT 0` 추가. 기존 캐릭터는 0에서 시작하며
소급 지급은 하지 않는다(레벨에 비례해 소급하면 밸런싱 기준선이 사라진다) — 이는
배포 공지 사항이다.

**검증**
- `skill_defs.rs` 부팅 테스트: `requiresSkill`이 존재하는 스킬인지, 순환 의존 없음,
  `id`가 `SkillId::from_str`로 파싱되는지.
- `game_state` 테스트: 사거리 밖 사용 거부, 쿨다운 중 사용 거부, 자원 부족 거부,
  이동으로 VCT 캔슬 시 자원 미소모, after-cast delay 중 다른 스킬 거부 + 평타 허용.
- 인게임: 스킬 사용 시 캐스팅 바와 서버 판정 시각이 어긋나지 않는지, 연타 시
  난사되지 않는지.
- 에이전트: 봇이 `use_skill`로 같은 스킬을 쓰고 같은 쿨다운을 받는지.

**성능** — 사용 시도당 락 3~4개(스킬 상태, 인벤토리, 몬스터, 허기). 새 틱 없음
(IMP-3.1의 지연 만료 판정). `SkillResult`는 AOI 팬아웃 대상이므로 페이로드를
최소로 유지한다 — 스킬 정의는 클라이언트가 CSV로 이미 갖고 있으므로 id만 보낸다.

---

### IMP-3.3. 방어 2단 구조 (Hard/Soft)

| | |
|---|---|
| 결정 | 채택(방어 성장축 도입 시) (09 #13) |
| 난이도 | 중 |
| 선행 조건 | IMP-3.2(피해 폭이 넓어진 뒤) |
| 프로토콜 변경 | 없음 |
| 저장 스키마 변경 | 없음 |

**손댈 파일**
- `data-src/items.csv` — `armorFlat`(감산), `armorPct`(비율) 두 컬럼.
- `server/src/item_defs.rs` — 두 필드.
- `server/src/game/combat.rs` — `pub fn apply_defense(damage, hard_pct, soft_flat) -> u32`.
- `server/src/game_state/combat.rs:181` `equipped_guard` 옆에
  `equipped_armor(&PlayerInventory) -> (u32 pct, u32 flat)`.
- `server/src/game_state/combat.rs:651` `broadcast_monster_attack` — 피해 적용 직전 통과.
- `doc/COMBAT.md` — 환산표.

**구현 방향**
**`guard`의 의미는 재정의하지 않는다.** doc 10 §3.2는 "guard의 의미를 재정의하되
환산표를 문서화한다"고 했지만, `guard`는 이미 명중 판정의 목표수(AC)로 코드 전체에
박혀 있다 — `resolves_as_hit`(`server/src/game/combat.rs:85`), `effective_guard`
(`game_state/combat.rs:193`), 클라이언트의 `GuardUpdated` 표시까지. 여기에 감산
의미를 겹치면 한 숫자가 두 일을 하게 되고, 환산표가 필요한 이유가 바로 그 겹침이다.
**축을 하나 더 만드는 쪽이 싸다**: `guard`는 "맞느냐"를, `armor`는 "얼마나 아프냐"를
담당한다. D&D의 AC와 RO의 DEF는 원래 다른 축이므로 이것이 원본에도 충실하다.
순서는 RO 그대로 **비율 먼저, 감산 나중, 최소 1**:

```
after_hard = damage × (100 − armor_pct) / 100
final      = max(1, after_hard − armor_flat)
```

비율만 있으면 고레벨 무한 방어가 되고, 감산만 있으면 다단·저피해 공격이 전부 0이
된다 — 이것이 2단을 쓰는 유일한 이유이며, IMP-3.2로 피해 폭이 넓어지기 **전에는
넣을 필요가 없다**(그래서 Phase 3 후반이다).
방어구 인챈트(`enchant`)는 지금처럼 `guard`에 붙는다 — `armor`에도 붙이면 한 번의
강화가 두 축을 올려 인챈트 사다리 밸런스가 무너진다.

**데이터 스키마**

| 컬럼 | 의미 | 빈 칸 |
|------|------|-------|
| `armorPct` | Hard 방어: 받는 피해 비율 감소(합산, 상한 60) | 0 |
| `armorFlat` | Soft 방어: 절대 감산(합산) | 0 |

**마이그레이션** — 없음. 빈 칸이 중립이라 데이터를 채우기 전까지 무변화.

**검증**
- `server/src/game/combat.rs` 테스트: 순서 뒤집기 시 결과가 달라지는지(순서가 스펙),
  최소 1 보장, `armorPct` 상한.
- 인게임: 같은 몬스터에게 방어구 유/무로 맞아 피해 분포 비교.

**성능** — 피격당 곱셈·뺄셈 각 1회. `equipped_armor`는 `equipped_guard`와 같은
순회이므로 한 번에 둘 다 계산해 반환하도록 합친다(순회 중복 방지).

---

### IMP-3.4. 경제 스킬 `Trading`

| | |
|---|---|
| 결정 | 채택(변형) (09 #15) |
| 난이도 | 중 |
| 선행 조건 | 없음 (IMP-3.2와 독립 — `SkillId` 확장만 공유) |
| 프로토콜 변경 | 없음 (`SkillsUpdate` / `SkillXpGained` 재사용) |
| 저장 스키마 변경 | 없음 (`character_skills` 재사용) |

**손댈 파일**
- `shared/src/skills.rs:13` — `SkillId::Trading` + 세 곳(`as_str`, `display_name`, `FromStr`).
- `shared/src/skills.rs` 옆 신규 `pub fn trade_rate_bonus_bp(level: u32) -> i32` —
  서버·클라·에이전트가 같은 숫자를 쓰도록 `shared`에 둔다.
- `server/src/game_state/trading.rs:558` `buy_item` / `:1073` `sell_item` — 가격 보정.
- `server/src/game_state/trading.rs` — 거래 성사 시 `Trading` XP 지급.
- `client/src/lib/components/CharacterPanel.svelte` — 스킬 목록에 자동 노출.

**구현 방향**
**CHA와 `Trading`의 역할을 코드 수준에서 분리한다.** 겹치면 두 축이 서로를 무의미하게
만든다(09 #27이 스탯 배분을 기각한 것과 같은 논리):

| 축 | 무엇을 움직이나 | 코드 위치 |
|----|-----------------|-----------|
| **CHA** | 흥정(haggle)의 **협상 폭** — `DEAL_BASE_HALF_BAND_PCT`(10) ~ `DEAL_MAX_HALF_BAND_PCT`(25) | `server/src/game_state/deals.rs:20~:31` |
| **`Trading` 스킬** | NPC의 **고정 환율** — `merchants.csv`의 `sellRatePercent`와 구매가 | `trading.rs:558`, `:1073` |

즉 CHA는 "얼마나 크게 깎을 수 있는 판을 여는가", 스킬은 "기본값 자체가 얼마인가"다.
둘은 곱해지되 서로 다른 숫자를 건드리므로 밸런싱이 독립적이다.
상한은 레벨 30에서 **판매가 +25% / 구매가 −15%**로 고정한다. 무제한 곱은 제니
생성기가 된다 — 특히 상인에게 사서 주민 NPC에게 파는 경로(`wishlistRatePercent`가
100을 넘는다)가 이미 존재하므로 상한 없이 스킬을 얹으면 즉시 무한 루프가 열린다.
`walletCap`이 이를 막고 있지만 상한은 두 겹이어야 한다.
XP는 거래 **성사 금액**이 아니라 **횟수**에 비례해 준다 — 금액 비례는 고가품 한 번의
자전거래로 만렙이 된다.

**데이터 스키마** — 없음. 상한 상수는 `shared/src/skills.rs`.

**마이그레이션** — 없음. 스킬 레벨 0(엔트리 없음) = 현행 환율.

**검증**
- `shared/src/skills.rs` 테스트: 레벨 0에서 보정 0, 레벨 30에서 정확히 상한.
- `game_state` 테스트: 상인 구매 → 주민 판매 왕복이 스킬 만렙에서도 순이익 음수인지
  (**이 테스트가 이 항목의 핵심**).
- 인게임: 스킬 레벨을 올려 가며 같은 아이템의 판매가 변화 확인.

**성능** — 거래당 정수 연산. 거래는 플레이어 행동당 1회.

---

### IMP-3.5. 고액 거래 수수료

| | |
|---|---|
| 결정 | 채택 (09 #14) |
| 난이도 | 소 |
| 선행 조건 | IMP-0.1(수수료 소각·임계액 확정). IMP-3.4와 같은 PR을 권장 — 두 항목 모두 가격 계산 지점을 건드린다 |
| 프로토콜 변경 | 있음(선택) — `ShopState`/거래 확인에 `fee: i64` 표시 필드 |
| 저장 스키마 변경 | 없음 |

**손댈 파일**
- `shared/src/messages.rs` 근처 신규 `pub fn trade_fee(amount: i64) -> i64` —
  `shared`에 두어야 클라이언트 프리뷰와 에이전트 판단이 서버와 같은 값을 쓴다.
- `shared/src/wasm_api.rs` — 게터 노출.
- `server/src/game_state/trading.rs:1073` `sell_item` / `:1336` `sell_items` — 지급액에서 차감.
- `client/src/lib/components/TradeWindow.svelte` — 수수료 표시.

**구현 방향**
**적용 대상은 플레이어 시장이지 NPC 상점이 아니다.** OpenMMO에는 RO식 노점 판매가
없다 — `shared/src/stall.rs`의 `Stall`은 위치·회전만 갖는 시각적 좌판이고 물건을
담지 않는다. 실제 플레이어 간 상거래는 **주민 NPC 거래 경로**
(`wishlist`/`stock`을 가진 캐릭터, `trading.rs`)를 탄다. 에이전트 NPC도 플레이어
캐릭터이므로 이 경로가 곧 이 게임의 플레이어 시장이다.
따라서 수수료는 **상대가 `merchants.csv`의 상인이 아닌 거래**에만 건다. 상인 상점은
이미 `sellRatePercent`(40/30%)라는 훨씬 큰 싱크를 갖고 있으므로 이중 과세다.
임계액 초과분에만 5%를 매긴다 — RO의 10,000,000z 대신 현재 경제 규모에 맞춘 값을
쓴다. **확인 필요:** 임계액은 `data-src/items.csv`의 `basePrice` 분포를 보고 정한다
(`python3 -c "import csv;print(sorted(int(r['basePrice']) for r in csv.DictReader(open('data-src/items.csv')) if r['basePrice']))"`).
초안은 `TRADE_FEE_THRESHOLD = 10_000`(구리), `TRADE_FEE_PCT = 5`.
**수수료는 소각한다.** 어느 지갑에도 입금하지 않는다 — NPC에게 가면 이전이지 싱크가
아니고, 주민 NPC의 `walletCap`을 우회하는 경로가 생긴다.

**데이터 스키마** — 없음. 상수 2개.

**마이그레이션** — 없음.

**검증**
- `shared` 테스트: 임계액 이하 0, 초과분에만 5%, 경계값, 음수 불가.
- `game_state` 테스트: 고액 거래 전후 **월드 총 통화량이 정확히 수수료만큼 감소**하는지.
- 인게임: 임계액 근처 거래에서 클라이언트 표시액과 실제 수령액 일치.

**성능** — 거래당 산술 1회.

---

### IMP-3.6. 코스튬 레이어

| | |
|---|---|
| 결정 | 채택(축소) (09 #18) |
| 난이도 | 중 |
| 선행 조건 | 없음 |
| 프로토콜 변경 | 있음 — `shared/src/entity.rs`의 `Player`에 `costume_head`/`costume_back` 2필드 **끝에 append** |
| 저장 스키마 변경 | 없음 (`character_items`의 `equip_slot`이 문자열이라 신규 슬롯을 그대로 담는다) |

**손댈 파일**
- `shared/src/inventory.rs:8` `EquipSlot` — `CostumeHead`, `CostumeBack` 추가.
  **4곳 전부**: 변형, `as_str`(:26), `FromStr`(:56), 그리고 테스트 테이블 `ALL_SLOTS`(:156).
- `shared/src/entity.rs` `Player` — 필드 2개를 **구조체 끝에** 추가
  (rmp_serde 위치 배열 계약 — 중간 삽입은 이후 모든 필드를 어긋나게 한다).
- `shared/src/lib.rs:78` — 버전 +1.
- `server/src/game_state/combat.rs:181` `equipped_guard` — **코스튬 슬롯 명시적 제외**.
- `server/src/game_state/inventory.rs` — 장착 시 `Player`의 코스튬 필드 갱신
  (`set_player_main_hand`과 같은 경로).
- `data-src/items.csv` — `category=costume` 행, `weight=0`, `guard` 없음.
- `client/src/lib/components/PlayerModel.svelte` — 본체 본에 `worldModel` 부착.
- `client/src/lib/network/networkTypes.ts` — `Player` 타입 확장.

**구현 방향**
슬롯은 **2개만**이다(머리·등). 09 #18의 "축소"는 애셋 투자 회수라는 목적을 최소
슬롯으로 달성한다는 뜻이고, 슬롯이 늘 때마다 `Player` 구조체와 AOI 스냅샷이 커진다.
"성능을 팔지 않는다"는 원칙은 문서가 아니라 코드로 보증한다: `equipped_guard`
(`combat.rs:181`)가 코스튬 슬롯을 `continue`로 건너뛰고, 그 사실을 테스트가 지킨다.
`ItemEffect`도 코스튬에 붙지 못하게 로드 시 assert한다.
원격 플레이어에게 외형이 보이려면 `Player` 구조체에 실려야 한다 — 이미
`main_hand: Option<String>`이 같은 방식으로 들어가 있으므로 패턴이 확립되어 있다.
`Option<String>`은 미착용 시 msgpack nil 1바이트라 5,000명 스냅샷에서도 부담이 없다.

**데이터 스키마**

```csv
id,name,description,weight,equipSlot,stackable,icon,worldModel,category,...
straw_hat,Straw Hat,A woven hat.,0,costume_head,false,straw_hat.png,costumes/straw_hat.glb,costume,...
```

**마이그레이션** — 없음. 기존 `character_items` 행은 그대로이고, 신규 슬롯 문자열은
`EquipSlot::from_str`이 인식하는 즉시 유효해진다.

**검증**
- `shared/src/inventory.rs`의 `equip_slot_str_roundtrip`(:174) — `ALL_SLOTS`에
  추가하면 자동으로 커버된다.
- `shared/src/lib.rs`의 `roundtrip_all_server_messages` — `Player` 필드 추가 후
  위치 배열이 어긋나지 않는지.
- `game_state` 테스트: **코스튬이 `effective_guard`를 바꾸지 않는지**.
- 인게임: 두 클라이언트로 접속해 상대의 코스튬이 보이는지, 재접속 후 유지되는지.

**성능** — `Player` 스냅샷에 미착용 시 2바이트. AOI 팬아웃은 이미 직렬화 1회 후
`Bytes` 공유이므로 인원수에 곱해지는 것은 전송량뿐이다.

---

### IMP-3.7. 업적 / 칭호

| | |
|---|---|
| 결정 | 채택 (12 §5) |
| 난이도 | 중 |
| 선행 조건 | **IMP-2.1**(보상은 전부 우편으로) |
| 프로토콜 변경 | 있음 — `ClientMessage::SetTitle`, `ServerMessage::AchievementUnlocked` / `TitleSet`, `Player`에 `title: Option<String>` 끝에 append |
| 저장 스키마 변경 | 있음 — `character_achievements` 테이블 + `characters.active_title` |

**손댈 파일**
- `data-src/achievements.csv`(신규).
- `server/src/achievement_defs.rs`(신규) — `titleId`/`rewardItem` 교차 검증.
- `server/src/auth.rs` — `ensure_achievement_schema`, `active_title` 컬럼
  (`CHARACTER_COLUMNS`:165 포함).
- `server/src/game_state/achievement.rs`(신규) — `bump(player, trigger, amount)` 단일 진입점.
- 카운터 훅(기존 이벤트 지점에만): `combat.rs:499`(처치),
  `game_state/fishing.rs`(대물), `game_state/dungeon.rs`(심층 도달),
  `server/src/housing/`(건축), `game_state/chat.rs`(공연).
- `shared/src/entity.rs` `Player` — `title` 필드 끝에 추가.
- `client/` — `AchievementPanel.svelte`, 토스트, 이름표·채팅에 칭호 표시.

**구현 방향**
평가는 **전적으로 이벤트 구동**이다. `bump()`가 카운터를 올리고 그 자리에서 임계와
비교한다 — 틱에서 5,000명 × 업적 수를 훑는 구조는 제약 (c) 체크리스트 1번 위반이다.
카운터는 두 종류로 갈린다: **누적형**(처치 수, 요리 횟수 — 저장 필요)과
**최고치형**(던전 최심층, 최대어 cm — 저장 필요)이지만, 둘 다 `character_achievements`에
해금 여부만 남기고 **진행 중 카운터는 메모리에 두지 않는다** — 대신 이미 저장 중인
값을 재사용한다(던전 발견은 `character_dungeon_discoveries`, 낚시 `trophyCm`은
기존 낚시 경로). 새 상태를 만들지 않는 것이 이 항목이 싼 이유다.
**확인 필요:** 누적 처치 수처럼 현재 어디에도 저장되지 않는 트리거가 필요하다면
그 카운터는 `characters`에 컬럼으로 추가한다 — 어떤 트리거가 기존 저장값으로
커버되는지는 `grep -rn "trophy_cm\|dungeon_discover\|chest_opens" server/src/`로 확인.
보상은 예외 없이 우편(IMP-2.1)이다.
칭호는 표시 전용이며 서버 비용이 거의 없다 — `Player`에 `Option<String>` 하나,
전투·경제에 어떤 영향도 주지 않는다. 이것이 12 문서가 "비용 대비 체류 시간 효과가
큰 축"이라 부른 이유다.

**데이터 스키마**

| 컬럼 | 의미 |
|------|------|
| `id` | 업적 id |
| `name` / `description` | 표시 |
| `trigger` | `monster_kill` / `fish_trophy` / `dungeon_depth` / `house_room` / `song_played` / `cook` |
| `triggerArg` | 트리거의 인자 (몬스터 id 등, 빈 칸 = 전체) |
| `threshold` | 임계값 |
| `titleId` | 해금되는 칭호 (빈 칸 = 칭호 없음) |
| `rewardItem` / `rewardZeny` | 우편으로 지급 |

```csv
id,name,description,trigger,triggerArg,threshold,titleId,rewardItem,rewardZeny
deep_delver,Deep Delver,Reach dungeon depth 10.,dungeon_depth,,10,Delver,,2000
angler_1,Angler,Land a trophy fish.,fish_trophy,,1,Angler,,500
```

```sql
CREATE TABLE IF NOT EXISTS character_achievements (
  character_id   INTEGER NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
  achievement_id TEXT NOT NULL,
  unlocked_at    INTEGER NOT NULL,
  PRIMARY KEY (character_id, achievement_id)
);
```

**마이그레이션** — 신규 테이블 + `active_title TEXT NULL`. 기존 캐릭터는 소급 해금
없음(소급하려면 과거 행동 기록이 필요한데 없다) — 배포 공지 사항.

**검증**
- 부팅 테스트: 없는 아이템/트리거를 참조하면 기동 실패.
- `game_state` 테스트: 임계 도달 시 정확히 한 번만 해금(중복 우편 금지),
  해금되지 않은 칭호로 `SetTitle` 시 거부.
- 인게임: 대물 낚시 → 토스트 → 우편 → 칭호 활성 → 이름표·채팅에 표시.

**성능** — 이벤트당 정수 비교 1~2회 + 해금 시에만 DB 쓰기. 칭호는 `Player` 스냅샷에
`Option<String>` 하나.

---

# Phase 4 — 사회 시스템 (부하 검증 선행)

| ID | 항목 | 09 # | 10 § |
|----|------|------|------|
| IMP-4.1 | 길드 | 21 | 4.1 |
| IMP-4.2 | 인스턴스 던전 | 20 | 4.2 |
| IMP-4.3 | 거점 점유 (공성전 대체) | 22 | 4.3 — **보류** |
| IMP-4.4 | 제작 성공률 공식 | 19 | — **조건부** |
| IMP-4.5 | 에이전트 NPC 동반자 계약 | 23 | — |
| IMP-4.6 | 제한형 매크로 | 39 (12 변형) | — |

---

### IMP-4.1. 길드

| | |
|---|---|
| 결정 | 변형 (09 #21 — 명단만으로는 유지되지 않는다) |
| 난이도 | 대 |
| 선행 조건 | IMP-2.3(창고 스키마·프로토콜 재사용), IMP-1.7(`$` 접두사) |
| 프로토콜 변경 | 있음 — 길드 CRUD 6종 + `GuildChat` + 길드 창고(IMP-2.3 메시지 재사용) |
| 저장 스키마 변경 | 있음 — `guilds` / `guild_members` / `guild_ranks` / `guild_storage` |

**손댈 파일**
- `server/src/auth.rs` — 테이블 4개 + `ensure_guild_schema`.
- `server/src/game_state/guild.rs`(신규), `server/src/game_state/mod.rs`에 필드 2개.
- `server/src/game_state/chat.rs:297` — `$` 라우팅.
- `server/src/housing/mod.rs` — `HouseData.owner_id`(`shared/src/housing.rs:150`)에
  길드 소유를 표현. **확인 필요:** 현재 `owner_id`가 계정인지 캐릭터인지
  (`grep -rn "owner_id" server/src/housing/ client/src/lib/managers/housingManager.ts`).
- `client/` — `GuildPanel.svelte`, `overlayStack.ts`, 채팅 탭.
- `agent-client/` — 길드 액션(에이전트도 길드원이 될 수 있어야 한다).

**구현 방향**
09 #21의 요지는 "명단은 채팅방에 그친다"이므로 최초 릴리스에 **길드 창고 + 길드
하우스** 둘을 함께 넣는다. 창고는 IMP-2.3의 스키마·프로토콜·원자성 패턴을 그대로
복제한다(슬롯 240, 접근 권한은 랭크 비트).
칭호=권한(RBAC) 구조는 RO 그대로 가져오되 **20개가 아니라 5개**로 시작한다 —
`guild_ranks(guild_id, rank_id, name, perm_bits)`, 비트는 초대/추방/창고입금/창고출금/
하우스편집 5개.
인원 상한은 **30 고정**이며 판매하지 않는다(09 #35 기각).
성능의 핵심은 조회 인덱스다(10 §4.1의 지적). `guild_members(character_id)`에
**UNIQUE 인덱스**를 걸어 "이 캐릭터의 길드"를 O(1)로 만들고, 런타임에는
`guild_of: HashMap<PlayerId, GuildId>`를 둔다. 길드 채팅은 파티 푸시와 동일하게
**멤버 순회가 아니라 dirty-set → 길드당 메시지 1회** 구조를 쓴다
(`tick_party_push`, `server/src/game_state/party.rs:822`가 그 참조 구현이다).

**데이터 스키마**

```sql
CREATE TABLE IF NOT EXISTS guilds (
  id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE,
  leader_character_id INTEGER NOT NULL, created_at INTEGER NOT NULL,
  house_id TEXT
);
CREATE TABLE IF NOT EXISTS guild_members (
  guild_id INTEGER NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
  character_id INTEGER NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
  rank_id INTEGER NOT NULL DEFAULT 0, joined_at INTEGER NOT NULL,
  PRIMARY KEY (guild_id, character_id)
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_guild_members_character
  ON guild_members(character_id);   -- 한 캐릭터는 한 길드
```

**마이그레이션** — 신규 테이블만.

**검증**
- `auth.rs` 테스트: 한 캐릭터가 두 길드에 들어가면 UNIQUE 위반, 길드장 탈퇴 처리,
  캐릭터 삭제 캐스케이드.
- `game_state` 테스트: 권한 없는 랭크의 창고 출금 거부, 30명 초과 초대 거부,
  길드 채팅이 비길드원에게 가지 않는지.
- 인게임: 길드 생성 → 초대 → 랭크 부여 → 창고 입출금 → 하우스 편집 권한 확인.

**성능** — 길드당 최대 30명, 조회는 인덱스 O(1). 길드 채팅은 길드당 1메시지.
길드 창고는 IMP-2.3과 같이 **열려 있을 때만 상주**한다.

---

### IMP-4.2. 인스턴스 던전

| | |
|---|---|
| 결정 | 변형 (09 #20 — 전체 사본 대신 시드 혼합) |
| 난이도 | 중~대 |
| 선행 조건 | IMP-0.1(개인 쿨다운 확정). IMP-4.1은 불필요(파티만으로 충분) |
| 프로토콜 변경 | 있음 — `ServerMessage::DungeonInstance { entrance_id, party_seed }` |
| 저장 스키마 변경 | 있음 — `character_instance_cooldowns` |

**손댈 파일**
- `shared/src/dungeon/mod.rs:368` `dungeon_seed` — `dungeon_seed_with(entrance_id, party_seed)`
  추가. **기존 `dungeon_seed(id) == dungeon_seed_with(id, 0)`을 보장한다.**
- `shared/src/dungeon/mod.rs:397` `generate_dungeon_for` — `_with` 변형 추가.
- `shared/src/dungeon/tests.rs` — 골든 해시는 `party_seed = 0`에서 **불변**이어야 한다.
- `shared/src/wasm_api.rs` — `DUNGEON_LAYOUTS` 메모이제이션 키를
  `entrance_id` → `(entrance_id, party_seed)`로 확장 + **상한 도입**.
- `server/src/game_state/dungeon.rs:82` `DungeonRuntime` — 인스턴스별 런타임 분리.
- `server/src/game_state/passability.rs:236` — 캐시 키 `dungeon:{id}` →
  `dungeon:{id}:{party_seed}`.
- `server/src/auth.rs` — 쿨다운 테이블.
- `client/src/lib/managers/dungeonManager.ts` — `party_seed` 보관 및 재생성.

**구현 방향**
전체 사본을 만들지 않는다. 던전은 이미 **입구 id의 FNV-1a 해시 → ChaCha8 시드**로
서버·wasm 클라이언트가 각각 동일하게 생성한다(네트워크로 지오메트리를 보내지
않는다). 여기에 파티 시드를 하나 더 먹이면 격리 효과를 공짜로 얻는다.
결정성 계약을 깨지 않는 것이 가장 중요하다: `dungeon_seed`는 손대지 말고
`dungeon_seed_with`를 추가해 `party_seed = 0`이 공용 던전이 되게 한다. 그러면
`shared/src/dungeon/tests.rs`의 골든 해시가 그대로 통과하고, 이미 배포된 클라이언트와의
공용 던전 호환성도 유지된다.
서버와 클라이언트가 같은 `party_seed`를 알아야 하므로 입장 시 푸시한다 —
`footprint_contains`로 입장 자격을 판정하는 기존 계약은 그대로다.
**쿨다운은 처음부터 개인 단위다**(09 #20). 파티 단위로 만들면 남의 인스턴스에
묻어가는 경로가 열리고, 나중에 개인 단위로 바꾸면 기존 파밍 루프를 깨뜨린다.

**데이터 스키마**

```sql
CREATE TABLE IF NOT EXISTS character_instance_cooldowns (
  character_id INTEGER NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
  entrance_id  TEXT NOT NULL,
  available_at INTEGER NOT NULL,
  PRIMARY KEY (character_id, entrance_id)
);
```
쿨다운 길이는 `data-src/dungeons.csv`에 `instanceCooldownSecs` 컬럼 추가(0 = 공용 던전).

**마이그레이션** — 신규 테이블 + CSV 컬럼. `instanceCooldownSecs`가 0/빈 칸인 던전은
지금과 완전히 동일하게 동작한다.

**검증**
- `shared/src/dungeon/tests.rs`: 골든 해시 불변, 서로 다른 `party_seed`가 서로 다른
  레이아웃을 만드는지, 같은 `party_seed`가 네이티브·wasm에서 동일한지.
- `game_state` 테스트: 쿨다운 중 입장 거부, 파티 해체 후 재입장 시 시드 처리.
- 인게임: 두 파티가 같은 입구로 들어가 서로 다른 미로를 보는지.

**성능** — **wasm `DUNGEON_LAYOUTS` 캐시 상한이 필수다.** 현재는 "레지스트리가 작다"는
전제로 무제한 메모이제이션이고, 파티 시드가 들어오면 키가 무한히 늘어난다.
LRU 4개로 제한한다(플레이어가 동시에 볼 수 있는 던전은 하나다).
서버 측 `PassabilityCache`도 같은 이유로 인스턴스가 비면 항목을 제거해야 한다 —
`leave_dungeon_floor`(`server/src/game_state/dungeon.rs`)의 층 비움 경로에 붙인다.

---

### IMP-4.3. 거점 점유 (공성전 대체) — 보류

| | |
|---|---|
| 결정 | 보류 (09 #22) |
| 난이도 | 대 |
| 선행 조건 | **SPK-1 밀집 전투 부하 테스트 go**, IMP-4.1 |
| 프로토콜 변경 | 미정 |
| 저장 스키마 변경 | 미정 |

**손댈 파일** — 지금 손대는 것은 **테스트뿐이다**.
- `server/src/game_state/tests/spawn_scale_tests.rs` 옆에
  `combat_scale_tests.rs`(신규, 기존 `spawn_scale_tests.rs` 명명 규칙) — 한 좌표에 N명을 모으고 AOI 팬아웃 메시지 수와
  틱 소요를 측정.

**구현 방향**
착수 조건을 숫자로 못 박는다: **반경 43m(`EVENT_DELIVERY_RADIUS`) 안에 200명**을
넣고 전투를 돌렸을 때, ① `tick_player_movement`(200ms) 한 회가 예산을 넘지 않고,
② 초당 팬아웃 메시지 수가 브로드캐스트 채널 용량(1000)을 유발하지 않으며,
③ `Lagged` 경고가 나지 않을 것. 이 셋을 통과하기 전에는 설계만 한다.
현재 구조상 밀집은 O(N²)로 자란다 — AOI 셀 크기가 곧 전달 반경이므로 한 셀에
200명이 모이면 이동 1회가 200건의 팬아웃이 되고, 200명이 동시에 움직이면 40,000건이다.
따라서 설계 방향은 "성을 만든다"가 아니라 **"밀집 상한과 시야 컬링을 먼저 만든다"**다.
형태는 하우징 부지·랜드마크 점유이며, 하우징이 이미 좌표 소유 개념을 갖고 있다.

**데이터 스키마** — 미정.
**마이그레이션** — 미정.
**검증** — 위 3개 조건이 곧 검증이다.
**성능** — 이 항목은 전체가 성능 문제다. 벤치가 산출물이다.

---

### IMP-4.4. 제작 성공률 공식 — 조건부

| | |
|---|---|
| 결정 | 채택(도입 시) (09 #19) |
| 난이도 | 중 |
| 선행 조건 | 제작 콘텐츠를 넣기로 결정할 것 |
| 프로토콜 변경 | 도입 시 있음 |
| 저장 스키마 변경 | 없음 |

**손댈 파일**(도입 시)
- `data-src/recipes.csv`(신규), `server/src/recipe_defs.rs`(신규).
- `server/src/game_state/inventory.rs` — 인챈트 롤 경로(:1021 부근) 재사용.

**구현 방향**
제작을 도입할 때 새 확률 표현을 만들지 않는다. 인챈트가 이미
**basis point(`ENCHANT_BP_SCALE = 10_000`) + 단일 사다리 함수** 구조이므로 제작도
`fn craft_success_bp(...) -> u32`로 같은 스케일을 쓴다. 플레이어가 규칙을 두 번
배우지 않아도 되는 것이 09 #19의 근거다.
공식은 RO의 "투자 − 욕심"을 3~18 스케일로 옮긴다:
`base_bp + dex_mod × K + skill_level × M − 옵션당 페널티`. 옵션(더 좋은 결과를 노리는
선택)이 성공률을 깎는 구조가 핵심이며, 이것이 인챈트의 "어디서 멈출지 스스로 정한다"와
동형이다.
**실패 시 재료 소실**로 통일한다 — 인챈트가 장비를 파괴하는 것과 같은 톤이다.

**데이터 스키마** — 도입 시 `recipes.csv`
(`id,output,inputs,baseSuccessBp,skillId,dexK,skillM`).
**마이그레이션** — 없음.
**검증** — 도입 시 성공률 경계 테스트 + 재료 원자성(부분 소모 금지) 테스트.
**성능** — 제작은 플레이어 행동당 1회. 재료 차감은 `inventories` 쓰기 락 1회.

---

### IMP-4.5. 에이전트 NPC 동반자 계약

| | |
|---|---|
| 결정 | 변형 (09 #23 — 펫 대신 에이전트 계약) |
| 난이도 | 중 |
| 선행 조건 | 없음 |
| 프로토콜 변경 | 있음 — `ClientMessage::HireCompanion { npc_player_id, hours }`, `ServerMessage::CompanionContract { npc_player_id, expires_at }` |
| 저장 스키마 변경 | 없음 (계약은 메모리 + 만료, 재시작 시 소멸 — 요금은 선불이므로 환불 정책을 문서화) |

**손댈 파일**
- `server/src/game_state/companion.rs`(신규) — 계약 맵과 만료.
- `server/src/main.rs:103` `time_sync_tick`(8초) — 만료 스윕을 **여기에 얹는다**(새 틱 금지).
- `data-src/npcs.csv` — `hireRatePerHour` 컬럼 추가(빈 칸 = 고용 불가).
- `agent-client/src/state/events.rs:264` `push_event` — `CompanionContract` 처리,
  `classify_event`(:21)에서 Urgent.
- `agent-client/src/driver/action.rs` — 기존 `follow` 액션 재사용(새 액션 불필요).
- `agent-client/data/npcs/<id>/instance.txt` — 계약 중 행동 규범.

**구현 방향**
09 #23의 요지대로 **기존 시스템의 확장으로 끝낸다.** 유지비는 먹이가 아니라 급여이고,
`npcs.csv`에 `salaryPerDay` / `walletCap`이 이미 있다. 새로 필요한 것은 요금 컬럼
하나와 계약 상태다.
계약 자체는 서버 판정이지만 **행동은 강제하지 않는다.** 서버는 계약 사실과 만료 시각만
관리하고, "따라가고 함께 싸운다"는 것은 NPC 에이전트의 프롬프트가 결정한다 —
`AgentAction::follow`가 이미 있으므로 새 액션이 없다. 이것이 제약 (c)와 맞물린다:
계약된 NPC도 여전히 같은 프로토콜만 쓰고, 사람이 다른 사람에게 "따라와 달라"고
부탁하는 것과 서버에서 구별되지 않는다.
용병의 30분 만료(08 문서)는 **LLM 비용 상한과 같은 문제**라는 관찰이 이 설계의
핵심이다. 실제 비용 상한은 `agent-client`의 `llm_scheduler`(`max_concurrent`)와
`always_active` 플래그가 이미 강제하고 있으므로, 계약 시간은 게임 내 표현일 뿐
새 비용 통제 장치를 만들 필요가 없다.
요금은 소각하지 않고 NPC 지갑에 넣는다 — 이 경우는 이전이 맞다(NPC의 급여 경제가
이미 존재하고 `walletCap`이 상한을 건다).

**데이터 스키마**

| 컬럼 | 의미 |
|------|------|
| `hireRatePerHour` | 시간당 계약 요금(구리). 빈 칸 = 고용 불가 |

**마이그레이션** — 없음(빈 칸이 기본).

**검증**
- `game_state` 테스트: 요금 부족 시 거부, 만료 시각 이후 계약 해제,
  이미 계약된 NPC의 중복 계약 거부, 서버 재시작 시 계약이 사라지는지(명시된 동작).
- 인게임: NPC 고용 → 따라오는지 → 만료 후 자기 일정으로 복귀하는지.

**성능** — 8초 틱에서 계약 맵(수십 건) 순회. 플레이어 전체 순회가 아니다.

---

### IMP-4.6. 제한형 매크로

| | |
|---|---|
| 결정 | 변형 (12 — 전투 자동화는 매크로 밖) |
| 난이도 | 소 |
| 선행 조건 | 없음 |
| 프로토콜 변경 | **없음** |
| 저장 스키마 변경 | 없음 (localStorage) |

**손댈 파일**
- `client/src/lib/stores/macroStore.ts`(신규) — `quickslotStore.ts`의 캐릭터별
  localStorage 패턴(`quickslots:<characterId>`)을 그대로 복제.
- `client/src/lib/components/MacroPanel.svelte`(신규), `GameHud.svelte`,
  `overlayStack.ts:24`에 `macros`(layer 0).
- `client/src/lib/components/FPSCounter.svelte:80` `handleKeydown` — `ALT+1`~`ALT+0`.

**구현 방향**
**서버를 전혀 건드리지 않는 것이 이 항목의 설계 그 자체다.** 매크로가 실행할 수
있는 것은 세 종류로 못 박는다: 이모트, 채팅 문구, UI 패널 열기.
전투 행동(공격·스킬·아이템 사용)을 배정할 수 없으므로 자동 전투 경로가 **코드
수준에서** 존재하지 않는다. 12 문서가 지적한 "사람이 봇처럼 노는" 경로에 대한 답이
정책이 아니라 구조인 셈이다.
이 프로젝트는 봇을 배제하지 않는다 — 문제는 공정성이 아니라 의도이고, 자동 전투를
원하면 `agent-client`를 쓰면 된다. 그쪽이 정직한 경로다.

**데이터 스키마** — 없음. `macros:<characterId>` localStorage 키.
**마이그레이션** — 없음.
**검증**
- vitest: 매크로에 전투 액션을 배정하려는 시도가 타입 수준에서 불가능한지
  (유니온 타입 3종).
- 인게임: `ALT+1`로 이모트, `ALT+2`로 문구, `ALT+3`으로 인벤토리 열기.
**성능** — 클라이언트 전용. 서버 부하 0.

---

## 기각 항목 — 다시 논의하지 않기 위한 기록

09 문서 §3의 기각 결정과, **그 대신 이 저장소에서 무엇을 하는가**.

| 09 # | 기각 항목 | 대신 무엇을 하는가 |
|------|-----------|--------------------|
| 24 | 속성 상성 10×10×4 | 몬스터에 `resist`/`weak` 태그(×0.5/×1.5)로 축소. 크기 축(IMP-1.5)이 먼저 들어가고, 태그는 그 다음 후보다 |
| 25 | 종족 축 10종 | 만들지 않는다. 몬스터 13종에 축을 하나 더 얹으면 각 칸이 빈다. 개성은 `hitDebuff`·`damageRoll`·`behavior`로 낸다 |
| 26 | 카드 / 슬롯 / 소켓 | 인챈트 주문서 종류 확대. 아이템 인스턴스에 소켓 상태를 저장하지 않으므로 `character_items` 스키마와 인벤토리 스냅샷 크기가 그대로다 |
| 27 | 스탯 포인트 배분 | 4d6 롤 + 총합 72 고정 유지(제약 (a)). 성장은 레벨·스킬 포인트(IMP-3.2)·장비로만 |
| 28 | 전직 트리 / 전생 / 3·4차 | "상한 해제" 아이디어만 IMP-3.2의 `requiresSkill`/`requiresSkillLevel` 해금 사다리로 차용 |
| 29 | ASPD 공식 | 공격 주기는 애니메이션 클립에 묶어 둔다(`claim_player_attack_window`, `player_anim_timing.csv`). 속도 보정이 필요하면 클립 배속 + 쿨다운 배율 |
| 30 | 섀도우 기어 | 장비 레이어를 늘리지 않는다. 외형은 코스튬(IMP-3.6), 성능은 인챈트로 분리 |
| 31 | 몬스터 SP / 마나 | 쿨다운만 쓴다(`attackCooldown`). 소유자 클라이언트 AI의 상태와 서버 검증 대상을 늘리지 않는다. 플레이어 스킬 자원도 같은 이유로 새 축 대신 허기(`costSatiation`)를 쓴다 |
| 32 | 맵 전환 로딩 | 이음매 없는 32km가 정체성이다(제약 (b)). 이동은 IMP-2.4의 고정 노드 텔레포트로만 |
| 33 | PvP 아이템 드랍 | 넣지 않는다. 에이전트-인간 동등성(제약 (c)) 아래에서 아이템 강탈은 봇 파밍 유인을 극대화한다 |
| 34 | 랜덤 박스 교환 | 넣지 않는다. 사행성은 별도 설계 판단이며, 카드 시스템(26)을 채택하지 않아 교환 대상도 없다 |
| 35 | 길드 인원 상한 판매 | IMP-4.1은 30명 **고정**. 과금과 성장 축을 섞지 않는다 |

**조건부 / 보류** (기각은 아니지만 지금 착수하지 않는 것)

| 항목 | 조건 |
|------|------|
| IMP-4.3 거점 점유 | 반경 43m 안 200명 밀집 전투 벤치 3개 조건 통과 |
| IMP-4.4 제작 | 제작 콘텐츠 도입을 결정할 것 |
| 캐시샵 (12 §6) | 넣는다면 **편의·외형만**(코스튬·창고 확장). 전투력 판매 없음, VIP 목적지 차등 없음(12에서 기각), 렌탈 장비 없음(만료 시각이 인벤토리·저장·UI 전반에 조건을 늘린다) |
| 존 기반 서버 스포너 | IMP-2.7이 CSV로 우회했다. `data/terrain/zones/*.json`의 `monsterSpawns`를 서버가 읽게 하려면 별도 과제 |
| 플레이어 간 직거래 / 우편 발신 | IMP-2.1은 시스템 발신만. 플레이어 발신은 사기·스팸·세탁 설계를 별도로 요구한다 |

---

## 착수 순서 요약

10 문서 §7은 IMP-1.1 → 헌팅 보드 둘만 권했다. 그러나 보상 지급 경로(우편)를 먼저
넣지 않으면 "인벤 만석"과 "오프라인" 두 예외를 퀘스트 보상 코드가 떠안는다
(12 §4). 그래서 순서를 하나 늘린다:

```
IMP-1.1  레벨 차 EXP 페널티        ─┐  "왜 다른 사냥터로 가야 하는가"
IMP-2.1  우편함                     │  (보상 경로 선행)
IMP-2.5  헌팅 보드  + IMP-2.6 일일 ─┘  "어디로 가야 하는가"
```

그 다음은 Phase 1의 나머지(1.2~1.7 — 전부 컬럼 하나, 서로 독립이라 병렬 가능),
이어서 도시 서비스 3종(IMP-2.2 → 2.3 → 2.4)이다.
Phase 3 착수 전에 **IMP-3.1을 반드시 먼저 확정**한다.

각 PR은 [doc/DEVELOPMENT.md](../DEVELOPMENT.md)의 규칙을 따르고, 커밋 전
`/preflight`(cargo fmt·clippy·test + build:wasm·vitest·svelte-check·eslint·prettier)를
돌린다. `shared/`를 건드린 PR은 WASM 재빌드가 필수이고, 프로토콜을 바꾼 PR은
서버·클라이언트 동시 배포가 전제다.

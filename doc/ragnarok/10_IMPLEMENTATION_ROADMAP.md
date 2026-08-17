# 10. 구현 로드맵

[09_OPENMMO_GAP_ANALYSIS](09_OPENMMO_GAP_ANALYSIS.md)의 채택 항목을 **바로 착수 가능한
작업**으로 분해했다. 각 항목에 손대야 할 파일이 적혀 있다. 환경 준비는
[doc/DEVELOPMENT.md](../DEVELOPMENT.md).

## 원칙

- **한 항목 = 한 PR.** 이 저장소는 CI가 Rust/클라이언트 양쪽을 다 돌린다.
- `shared/`를 건드리면 서버·클라이언트·에이전트 셋 다 영향을 받는다. WASM 재빌드 필수.
- 데이터로 표현할 수 있으면 **코드가 아니라 CSV로** 넣는다. 이 프로젝트의 기존 관성이다.
- 프로토콜 변경은 서버·클라이언트 동시 배포가 전제다
  ([add-protocol-message 스킬](../../.claude/skills/add-protocol-message/SKILL.md)).

---

## Phase 1 — 컬럼 하나로 끝나는 것들

가장 먼저 하는 이유: 새 시스템이 아니라 **기존 시스템의 빈칸 채우기**라서
리스크가 낮고, 뒤 단계의 밸런싱 기준선을 만들어 준다.

### 1.1 레벨 차 EXP 페널티
- `shared/src/xp.rs`: `monster_xp(level, guard)` → 플레이어 레벨을 받는 시그니처로.
  감쇠 곡선은 "±N 레벨 이내 100%, 그 밖은 선형/계단 감소" 형태로 시작.
- 호출부: `server/src/game_state/monster.rs`(처치 보상), `party_xp_share` 경로.
- 테스트: 기존 `xp.rs` 테스트 옆에 경계값 추가. 파티 분배가 솔로를 넘지 않는
  기존 불변식(`party_share_never_beats_soloing`)을 깨지 않는지 확인.

### 1.2 보스 프로토콜
- `data-src/monsters.csv`의 `boss` 컬럼 의미 확장: 디버프 면역 + 넉백 면역.
- `server/src/game_state/debuff.rs`에서 대상이 보스면 부여 스킵.
- 문서: [doc/DEBUFF.md](../DEBUFF.md)에 예외 규칙 한 줄.

### 1.3 디버프 저항 스탯
- `data-src/debuffs.csv`에 `resistStat`(예: `con`, `wis`) 컬럼 추가.
- `server/src/debuff_defs.rs`에 필드, `game_state/debuff.rs`에서 확률 보정.
  3~18 스케일이므로 `chance × (1 − (stat − 10) × k)` 같은 완만한 형태.
- 문서: [doc/DEBUFF.md](../DEBUFF.md) 표 갱신.

### 1.4 루터 몬스터
- `monsters.csv`의 `behavior`에 `looter` 추가.
- 몬스터 AI는 **소유자 클라이언트**에서 돈다 → `client/src/lib/managers/monsterManager.ts`.
  줍기·드랍은 반드시 **서버 검증**(`game_state/inventory.rs`의 바닥 아이템 경로 재사용).
- 층 인식 바닥 아이템 규칙을 그대로 따를 것 (2층에서 떨어진 것은 2층에서만).

### 1.5 크기 축
- `monsters.csv`에 `size`(small/medium/large), `items.csv`에 무기별 크기 배율 3열
  또는 `sizeMult` 문자열 1열(`1.0|0.75|1.25`).
- `server/src/game/combat.rs`에서 데미지 계산에 곱. 클라이언트는 표시만.

---

## Phase 2 — 월드를 쓰게 만드는 것들

Phase 1이 "왜 다른 곳으로 가야 하는가"를 만들었다면, 여기서 "어디로, 어떻게"를 만든다.

### 2.1 도시 서비스 NPC (세이브 · 창고 · 유료 이동)
가장 큰 항목이라 셋으로 쪼갠다.

**(a) 세이브 포인트 + 리스폰**
- `shared/src/character.rs`의 저장 레코드에 `save_point: Position`.
- 프로토콜: `ClientMessage::SetSavePoint`, 사망 처리 경로에서 사용.
- 기본값은 현재 리스폰 규칙 유지 — 마이그레이션 시 기존 캐릭터가 깨지지 않게.

**(b) 창고 (Storage)**
- `shared/src/inventory.rs`에 창고 컨테이너. 슬롯 상한을 **명시적으로** 정한다.
- 프로토콜: 열기/입금/출금/닫기. **전체 스냅샷이 아니라 델타**로 보낼 것 —
  5,000명 × 수백 슬롯을 스냅샷으로 밀면 대역폭이 즉시 문제가 된다.
- 서버 검증: 거리(도시 NPC 근처), 무게, 원자성(중복 생성 방지). 기존 거래
  (`game_state/trading.rs`)의 원자성 패턴을 재사용한다.
- 저장은 기존 **배치 세이브**에 합류시킨다. 새 디스크 IO 경로를 만들지 말 것.

**(c) 유료 이동 + "던전 워프 불가"**
- 목적지는 **도시 소수 고정 지점**만. 임의 좌표 텔레포트 금지.
- 도착 시 지형/오브젝트 스트리밍이 몰리므로 타일 캐시 예열과 함께 설계
  ([doc/LOADING_OPTIMIZATION.md](../LOADING_OPTIMIZATION.md)).
- 요금은 제니 싱크. 던전 입구/내부는 목적지에서 제외.

### 2.2 헌팅 보드 반복 퀘스트
- 신규 `data-src/hunting_quests.csv`:
  `id,name,monsterId,count,minLevel,maxLevel,rewardXp,rewardZeny,dailyLimit`.
- 진행 상태는 캐릭터 레코드에 `HashMap<questId, progress>` + 일일 카운터.
  **일일 한도는 1일차부터 넣는다** (09 문서 §4의 3번 기준).
- 몬스터 처치 훅은 이미 있다(XP 지급 지점) — 거기서 카운터 증가.
- UI: `client/src/lib/components/`에 패널, 오버레이는 `overlayStack.ts`에 등록.

### 2.3 미니보스
- 존 스폰 JSON에 `respawnVarianceSecs` 추가, `maxTotal: 1`과 조합.
- 맵 에디터(존 편집 UI)에도 필드 노출 — 데이터만 넣고 편집기를 빼먹으면
  운영에서 손으로 JSON을 고치게 된다.

### 2.4 MVP 기여도 보너스
- `game_state/combat.rs`에 몬스터별 누적 피해 기록(보스 한정, 메모리 상한 필수).
- 처치 시 최대 기여자에게 추가 보상. 파티 분배와 **별개 경로**.

---

## Phase 3 — 새 성장 축

### 3.1 전투 스킬 시스템 (선결 항목 있음)
**착수 전에 반드시 확정**: 시간 4분할(VCT / FCT / after-cast delay / cooldown).
이걸 나중에 넣으면 모든 스킬 정의와 애니메이션 타이밍을 다시 잡아야 한다.

- `shared/src/skills.rs`: `SkillId` 확장, 스킬 포인트 개념 도입
  (Job Level 대용 — 전투 참여로 획득).
- 스킬 정의는 CSV(`data-src/skills.csv`)로: 시전 시간 2종, 딜레이, 쿨다운, 자원 소모,
  사거리, 대상, 효과.
- 애니메이션은 기존 파이프라인 재사용 (`attackImpactDelay`와 같은 접근).
- 프로토콜: `ClientMessage::UseSkill`, `ServerMessage::SkillResult`.
  **서버가 쿨다운·사거리·자원을 판정한다.** 클라이언트 예측 금지.

### 3.2 방어 2단 구조
- 스킬로 데미지 폭이 넓어진 뒤에 도입한다 (그 전에는 감산만으로 충분).
- `game/combat.rs`: 비율 감소 → 감산 순서. `guard`의 의미를 재정의하되
  기존 밸런스가 깨지지 않도록 환산표를 문서화한다.

### 3.3 경제 스킬
- `SkillId::Trading` 추가. 기존 haggle(`ActiveDeal`)·상인 `sellRatePercent`에 곱.
- CHA와 함께 작동 — 두 축이 겹치지 않게 역할 분리를 먼저 문서화한다.

### 3.4 고액 거래 수수료
- 노점(`shared/src/stall.rs`)·거래 경로에 임계액 초과분 수수료.
- 수수료는 **소각**한다 (NPC에게 가면 싱크가 아니다).

---

## Phase 4 — 사회 시스템 (부하 검증 선행)

### 4.1 길드
- 명단 + 길드 창고 + 길드 하우스(기존 하우징 재사용)로 시작.
- 조회 경로는 접속자 전체 순회가 되지 않도록 인덱스 필수.

### 4.2 인스턴스 던전
- 던전 시드에 파티 ID를 섞는 방식. 전체 사본 생성 없이 격리 효과.
- 쿨다운은 **처음부터 개인 단위**.

### 4.3 거점 점유 (공성전 대체) — 보류
착수 조건: 한 지점 밀집 전투의 부하 테스트 통과. 그 전에는 설계만.

---

## 5. 5,000 동접 체크리스트

새 시스템을 넣을 때마다 이 다섯 개를 확인한다
([doc/DEVELOPMENT.md](../DEVELOPMENT.md) §8, [RUNTIME_PERFORMANCE.md](../RUNTIME_PERFORMANCE.md)).

1. 서버 틱에 플레이어 전체 순회가 새로 생기지 않는가.
2. 브로드캐스트가 관심 영역으로 잘려 있는가. 전체 스냅샷 대신 델타인가.
3. 락을 쥔 채 직렬화·IO·할당을 하지 않는가.
4. 새 디스크 쓰기 경로 대신 기존 배치 세이브에 합류했는가.
5. 클라이언트에 매 프레임 새 오브젝트/머티리얼이 생기지 않는가.

특히 **창고·퀘스트 진행·길드**는 캐릭터당 상태가 늘어나는 항목이다.
5,000명 × 상태 크기를 곱해 보고 들어간다.

---

## 6. 미수집 데이터를 마저 가져오는 절차

이 묶음은 **시스템 구조**만 담고 있다. 개별 아이템/몬스터/스킬/맵 수치는
`db.irowiki.org`에 있으나 이 환경의 egress 정책이 `irowiki.org`를 차단해
수집하지 못했다 (CONNECT 403).

필요해지면:

1. 환경의 네트워크 정책에 `irowiki.org`, `db.irowiki.org`를 허용 목록으로 추가한
   세션에서 진행한다 (환경 설정 문서:
   https://code.claude.com/docs/en/claude-code-on-the-web).
2. 확인 명령: `curl -sS -o /dev/null -w "%{http_code}" https://irowiki.org/wiki/Main_Page`
   — 200이 아니면 여전히 차단 상태다.
3. 수집 대상은 **표가 아니라 분포**다. 개별 수치를 그대로 옮기면 안 된다
   (RO 레벨 1~260 ↔ OpenMMO의 짧은 레벨 대역). 필요한 것은
   "레벨 대비 HP/ATK/드랍률이 어떤 곡선을 그리는가"이고, 그 곡선을 OpenMMO
   스케일로 다시 그린 뒤 `data-src/*.csv`에 넣는다.
4. 저작권: iRO Wiki 본문·데이터를 저장소에 그대로 커밋하지 않는다. 파생 수치와
   출처 링크만 남긴다. 애셋을 함께 들여올 경우 `doc/assets/` 기록 규칙을 따른다.

---

## 7. 착수 추천

**지금 바로 시작한다면 1.1(레벨 차 EXP 페널티) → 2.2(헌팅 보드) 순서.**
둘이 짝을 이뤄 "왜 다른 사냥터로 가야 하는가 + 어디로 가야 하는가"를 완성하고,
합쳐도 신규 시스템 하나 분량이며, 프로토콜 변경이 최소다.

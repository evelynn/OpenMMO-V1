# 헌팅 보드 (Hunting Board)

도시 게시판에서 "몬스터 N마리 처치" 계약을 받아 사냥하고 반납하는 반복 콘텐츠.
설계 근거는 [ragnarok/08_CONTENT_LOOPS.md](ragnarok/08_CONTENT_LOOPS.md) §1,
구현 방향은 [ragnarok/13_IMPLEMENTATION_DIRECTION.md](ragnarok/13_IMPLEMENTATION_DIRECTION.md)
IMP-2.5 · IMP-2.6.

## 계약 정의

[data-src/hunting_quests.csv](../data-src/hunting_quests.csv) 한 줄이 계약 하나다.

| 컬럼 | 뜻 |
|------|-----|
| `id` | 계약 식별자. 부팅 시 `u16`으로 인턴된다 |
| `boardId` | 걸리는 게시판 (현재 `capital` 하나) |
| `name` | 표시 이름 |
| `monsterId` | 대상 몬스터 (`monsters.csv`에 있어야 한다) |
| `count` | 필요 처치 수 |
| `minLevel` / `maxLevel` | 수락 가능 레벨 구간 |
| `rewardXp` / `rewardZeny` | 보상 |
| `rewardItem` | 보상 아이템 (`items.csv`에 있어야 한다. 빈 칸 = 없음) |
| `dailyLimit` | 하루 완료 상한. **0 = 무제한** |

없는 몬스터나 아이템을 참조하면 **서버가 부팅하지 않는다**
(`server/src/quest_defs.rs`). 오타 난 계약이 조용히 완료 불가 상태로 남는 것보다
기동 실패가 낫다.

## 규칙

- **동시 수락 5개** (`MAX_ACCEPTED_QUESTS`). 초과하면 거부한다.
- **레벨 구간 밖에서는 수락할 수 없다.** 레벨 차 EXP 감쇠([COMBAT.md](COMBAT.md))와
  짝을 이루는 장치다 — 구간 안의 계약이 곧 그 레벨의 사냥터다.
- **진척은 XP를 나눠 받는 사람 전원에게** 들어간다. 파티원이 처치 기여를 인정받지
  못하면 파티 플레이가 벌점이 된다.
- **보상은 우편으로 간다** ([12_UX_SERVICES](ragnarok/12_UX_SERVICES.md) §4).
  가방이 가득 차 있어도 보상이 증발하지 않는다.
- **포기하면 진척은 0으로 돌아가지만, 그날 쓴 일일 횟수는 돌아오지 않는다.**

## 일일 한도 리셋

**기준은 게임 시계가 아니라 실시간 UTC 날짜다** (`day_key = unix_secs / 86400`).

이 게임의 하루는 실시간 3시간이므로(`server/src/game_state/time.rs`의
`REAL_DAY_DURATION_SECONDS`), 게임 자정을 리셋으로 쓰면 "일일" 계약이 하루에 여덟 번
풀린다. 그러면 한도를 건 이유 자체가 사라진다.

판정은 **반납 시점 한 번**만 한다 — 저장된 `day_key`가 오늘과 다르면 카운트를 0으로
되돌리고 진행한다. 자정 스윕 틱도, 로그인 시 전체 순회도 없다.

## 데이터가 하는 일: 고효율

`dailyLimit > 0`인 계약은 무제한 계약보다 **보상을 3~5배**로 잡는다. "하루 한 번만
할 수 있지만 확실히 이득인 것"이 일일 콘텐츠의 존재 이유이고, 이 배율은 1일차부터
들어가야 한다. 나중에 올리는 것은 밸런스 조정이지만, **나중에 한도를 새로 거는 것은
너프로 읽힌다** ([09_OPENMMO_GAP_ANALYSIS](ragnarok/09_OPENMMO_GAP_ANALYSIS.md) §4).

## 구현 위치

| 층 | 파일 |
|----|------|
| 정의 로드·교차검증 | `server/src/quest_defs.rs` |
| 저장 | `server/src/auth.rs` (`character_quests`), 배치 세이브 합류 |
| 상태 전이 | `server/src/game_state/quest.rs` |
| 처치 훅 | `server/src/game_state/combat.rs` (XP 수령자 목록 재사용) |
| 프로토콜 | `shared/src/messages.rs` (`OpenQuestBoard`/`AcceptQuest`/`AbandonQuest`/`TurnInQuest` → `QuestBoard`/`QuestAccepted`/`QuestProgress`/`QuestCompleted`) |
| 클라이언트 | `stores/questStore.ts`, `QuestBoardPanel.svelte`, `QuestTracker.svelte` |
| 에이전트 | `quest_board` / `accept_quest` / `turn_in_quest` 액션 |

## 성능

처치당 비용은 수령자당 최대 5회 비교이고 할당이 없다. 진행 푸시는 **변화가 있을 때
해당 플레이어에게만** 간다 — 브로드캐스트하지 않는다. 5,000명 × 수락 5개 =
`(u16, u16)` 25,000개, 100KB 수준. **틱에서의 전체 순회는 없다.**

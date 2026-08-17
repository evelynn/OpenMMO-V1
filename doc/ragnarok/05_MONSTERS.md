# 05. 몬스터 (Monsters)

출처: [Monster](https://irowiki.org/wiki/Monster) · [MVP](https://irowiki.org/wiki/MVP) ·
[Boss Protocol](https://irowiki.org/wiki/Boss_Protocol) ·
[Leveling Spots](https://irowiki.org/wiki/Leveling_Spots) ·
[iW Database — Monster Info](https://db.irowiki.org/db/monster-info/)

## 1. 행동 유형

| 유형 | 행동 |
|------|------|
| **Passive** | 공격받기 전까지 무시 |
| **Aggressive** | 시야에 들어온 대상을 먼저 공격 |
| **Looter** | 바닥의 아이템을 주워간다. **죽으면 주운 것을 떨어뜨린다** |
| **Assist** | 같은 종류가 맞으면 함께 반응 |

루터는 특히 눈여겨볼 만하다. "드랍템을 방치하면 몹이 가져간다"는 규칙 하나로
바닥 아이템에 시간 압박을 만들고, 그 몹을 잡으면 회수되므로 좌절이 아니라
추격의 동기가 된다.

## 2. 등급

| 등급 | 특징 |
|------|------|
| 일반 | 즉시 리스폰. 맵별 정원 유지 |
| **미니보스** | 특정 맵에 소수만 존재. 리스폰 **10분~2시간**. 강한 스킬·높은 HP. **FFA 아님** |
| **MVP** | 최상위. 처치 후 **일정 시간 + 최대 10분 변량** 뒤 리스폰. 슬레이브(하수인) 3~9마리 소환 |

- MVP는 **FFA(선착순 자유경쟁)** — 리스폰 시각 추적 자체가 콘텐츠가 된다.
- MVP/보스에는 **보스 프로토콜**이 붙는다: 상태이상·넉백 무효, 은신 감지
  ([03_MODIFIERS](03_MODIFIERS.md) §4).
- MVP 처치 시 기여도 1위에게 **MVP 보너스**(추가 EXP/드랍)가 별도로 간다 —
  파티 분배와 별개의 개인 보상 축.

## 3. 스폰 설계

- 일반 몬스터는 맵마다 **종류별 정원**이 정해져 있고 죽으면 즉시 채워진다.
- 미니보스/MVP만 시간 기반 리스폰 + 변량. 변량이 있어야 "타이머 알람 파밍"이
  완전 자동화되지 않는다.
- 사냥터 문서는 맵의 몬스터 레벨 분포로부터 적정 레벨대를 계산한다
  (최고 레벨 몹 −15 ~ 최저 레벨 몹 +20). EXP 레벨 차 페널티
  ([01_PROGRESSION](01_PROGRESSION.md) §3)와 짝을 이루는 설계다.

## 4. 몬스터 데이터 축

iW Database의 몬스터 레코드는 대략 다음을 갖는다: HP/SP, 레벨, ATK 범위, DEF/MDEF,
6스탯, 이동속도·공격속도·공격 딜레이, 시야/추적 범위, **속성(+레벨)·종족·크기**,
드랍 테이블(아이템별 %), 소환 슬레이브, 사용 스킬, 리스폰 정보.

여기서 OpenMMO가 아직 갖지 않은 축은 **속성/종족/크기, SP, 사용 스킬, 슬레이브 소환**이다.

## OpenMMO 적용

**현재 상태** (`data-src/monsters.csv`, 40개 컬럼)
- 이미 풍부하다: `health`, `level`, `guard`, `attackBonus`, `damageRoll`,
  `walkSpeed`/`runSpeed`, `attackRange`/`chaseRange`, `attackCooldown`,
  `attackImpactDelay`, `behavior`, 애니메이션 클립 8종, `weapon`/`weaponBone`/
  `weaponOffset`/`weaponDropChance`, `material`, `scale`, `boss`, `hitDebuff`,
  던전 스폰용 `dungeonMinDepth`/`MaxDepth`/`Weight`/`Aggressive`.
- 지상 스폰은 **플레이어를 따라다닌다.** `data-src/world.json`의 `ambientSpawns`와
  인당 캡(`maxMonstersPerPlayer = 30`)을 `tick_monster_spawns`
  (`server/src/game_state/monster.rs:854`)가 돌리고, 서버가 각 플레이어 주변에 스폰을
  요청하면 소유자 클라이언트가 유효 위치를 고른다. 몬스터 자신의 레벨이 스폰 게이트다
  (`min_ambient_player_level`).
- 맵 에디터가 그리는 사각형(`data/terrain/zones/`)의 `monsterSpawns` 배열은 **서버가
  읽지 않는다** — `server/src/world_config.rs:93`은 `noSpawnZones`만 읽는다.
  즉 **RO의 "맵별 정원"에 해당하는 것이 아직 없다.** 고정 사냥터가 없으므로
  레벨 차 페널티는 "어디로 갈까"보다 "주변 중 무엇을 잡을까"를 먼저 바꾼다.
- **몬스터 AI는 소유자 클라이언트에서 돈다** (`monsterManager.ai_tick_brain`).
  이것이 5,000 동접을 견디는 구조적 선택이며, 아래 판단의 전제다.

**가져올 것**
1. **루터(Looter) 행동.** `behavior` 컬럼에 값 하나 추가로 구현 가능하고,
   OpenMMO에는 이미 바닥 아이템 시스템(층 인식 포함)이 있다. 재미 대비 비용이
   가장 좋은 항목.
2. **미니보스 = 시간 기반 리스폰 + 변량.** 현재 앰비언트 스폰에는 주기도 변량도 없다 —
   `world.json`의 항목은 `monsterType`과 `maxDistance`뿐이고, 몹은 플레이어 주위에
   계속 채워진다. 고정 좌표에 장주기 + 변량으로 뜨는 개체를 넣으면 월드에
   **찾아다닐 목표**가 생긴다. 존 JSON이 아니라 `data-src/world_bosses.csv`
   (좌표 + `respawnBaseSecs`/`respawnVarianceSecs`) + 30초 틱으로 간다
   (13 IMP-2.7) — 존 사각형은 서버가 읽지 않기 때문이다.
3. **MVP 기여도 보너스.** 파티 분배(`party_xp_share`)와 별개로 최대 기여자에게
   추가 보상. 보스전이 "누가 막타 쳤나"가 아니라 "누가 기여했나"가 되게 한다.
4. **보스 프로토콜 플래그.** `boss` 컬럼에 디버프·넉백 면역을 묶는다 (03 문서와 동일 결론).

**변형할 것**
- 슬레이브 소환은 매력적이지만 소유자 클라이언트 AI 모델에서 "소환된 몹의 소유자는
  누구인가"를 먼저 정해야 한다. 보스와 같은 소유자로 묶고, **소환 총량을 하드 캡**
  (예: 동시 6)으로 제한한 뒤 도입한다.
- 몬스터 스킬은 스킬 시스템([02_STATS_COMBAT](02_STATS_COMBAT.md) §7) 이후로 미룬다.
  그 전까지는 `hitDebuff`와 `damageRoll` 변형으로 개성을 낸다.

**기각할 것**
- 몬스터에 SP/마나 자원. 클라이언트 AI가 관리해야 하는 상태가 늘어나고, 서버 검증
  대상도 늘어난다. 쿨다운만으로 같은 리듬을 만들 수 있다.
- 속성/종족 축 부여 (03 문서와 동일 사유, 크기 축만 채택).

**성능 주의**
- 스폰 정원을 올리는 변경은 5,000 동접에서 **모든 소유자 클라이언트의 AI 부하**와
  브로드캐스트 트래픽에 동시에 곱해진다. 현재 소유자별 스폰 캡은 O(1) 인덱스로
  강제된다 (`server/src/game_state/monster.rs`) — 새 스폰 유형을 넣을 때 이 캡을
  우회하지 말 것. 던전/관리자 스폰이 캡을 면제받는 이유와 그 대가를 먼저 읽는다.

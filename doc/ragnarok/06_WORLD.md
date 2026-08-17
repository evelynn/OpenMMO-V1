# 06. 월드와 던전 (World, Maps, Instances)

출처: [Places](https://irowiki.org/wiki/Places) · [Prontera](https://irowiki.org/wiki/Prontera) ·
[Payon](https://irowiki.org/wiki/Payon) · [Kafra](https://irowiki.org/wiki/Kafra) ·
[Instance](https://irowiki.org/wiki/Instance) ·
[Endless Tower](https://irowiki.org/wiki/Endless_Tower) ·
[Old Glast Heim](https://irowiki.org/wiki/Old_Glast_Heim) ·
[iW Database — Dungeon Map](https://db.irowiki.org/db/dungeon-map/)

## 1. 공간의 3분류

| 종류 | 성격 |
|------|------|
| **도시(Town)** | NPC 서비스 집결지. 몬스터 없음. 상점·창고·전직·워프의 허브 |
| **필드(Field)** | 도시 밖 야외. **워프 포탈로 접근 가능**. 사냥터 |
| **던전(Dungeon)** | 워프 포탈로 갈 수 없음. 몬스터 밀도 높음. 층 구조 |

핵심은 "**던전은 워프로 못 간다**"는 규칙이다. 이 한 줄이 던전을 목적지로 만들고,
이동 자체를 콘텐츠로 만든다. 필드는 워프로 건너뛸 수 있으니 소모품 취급.

도시는 저마다 정체성을 갖는다 (프론테라 = 수도, 페이욘 = 특정 직업의 본거지 등).
"모든 도시가 같은 서비스를 제공하되 하나씩 특별한 것을 갖는다"가 패턴이다.

## 2. 맵 단위 세계

RO의 월드는 **개별 맵 파일의 그래프**다 (`prt_fild05` 같은 맵 ID). 맵 경계를 밟으면
다음 맵으로 전환된다. 몬스터 정원·스폰·경험치 계산이 전부 맵 단위로 정의된다.

OpenMMO는 정반대로 **32km × 32km 이음매 없는 단일 월드**다
([doc/MAP_DESIGN.md](../MAP_DESIGN.md), [TERRAIN_GENERATION.md](../TERRAIN_GENERATION.md)).
따라서 RO의 맵 단위 규칙은 그대로 옮길 수 없고, **구역(zone) 단위**로 번역해야 한다.
OpenMMO에는 이미 그 그릇이 있다 — `data/terrain/zones/`의 사각형
([doc/ZONE_SYSTEM.md](../ZONE_SYSTEM.md)).

## 3. 카프라 서비스 (Kafra)

도시마다 있는 NPC가 제공하는 묶음:

| 서비스 | 내용 |
|--------|------|
| **Save** | 사망 시 리스폰할 세이브 포인트 지정 |
| **Storage** | 600슬롯 창고 접근 |
| **Teleport** | 다른 도시로 유료 이동 (예: 프론테라 → 게펜 2,000z) |
| **Cart rental** | 상인 카트 대여 |

**세이브·창고·이동이 한 NPC에 묶여 있다**는 점이 중요하다. 플레이어는 도시에
들어오면 이 NPC 하나에서 정비를 마치고 나간다. 유료 이동은 제니 싱크이면서
동시에 "돈으로 시간을 사는" 선택지다.

## 4. 인스턴스 (메모리얼 던전)

파티마다 **독립된 사본**이 생성되는 던전.

| 인스턴스 | 쿨다운 |
|----------|--------|
| Old Glast Heim | 16시간 |
| Advanced Old Glast Heim | 70시간 |
| Endless Tower | 6일 20시간 |

- 쿨다운은 **파티 단위가 아니라 입장한 멤버 개인별로** 시작된다. 남의 인스턴스에
  묻어가는 것을 막는 장치.
- 쿨다운이 길수록 보상이 크다. 일일 루프(§08)와 주간 루프를 분리하는 장치다.

## OpenMMO 적용

**현재 상태**
- 단일 이음매 없는 월드 + 절차적 지형. 도시는 절차적으로 배치된 정착지이며,
  마을은 `noSpawnZones`로 몬스터가 배제된다.
- 던전은 이미 있다: `data-src/dungeons.csv` (입구 좌표, 층수, 보스, 상자 티어,
  입구 방향), 시드 결정적 미로, 최대 20층. 지오메트리는 하우징 절차 생성 재사용.
- 하우징이 있어 플레이어가 월드에 영구 구조물을 짓는다.
- 워프/텔레포트/세이브 포인트/창고는 **없다**. 관리자 명령 `/summon`·`/goto`만 존재.

**가져올 것**
1. **카프라형 도시 서비스 NPC.** OpenMMO의 도시는 현재 지나가는 장소에 가깝다.
   세이브 포인트 + 창고 + 유료 이동을 **한 NPC에 묶어** 도시마다 배치하면,
   32km 월드에서 도시가 비로소 기능적 거점이 된다. NPC 시스템
   (`data-src/npcs.csv`, `npc_schedule`)이 이미 있어 붙일 자리가 명확하다.
2. **"던전은 워프 불가" 규칙.** 유료 이동을 넣을 때 반드시 함께 넣어야 하는 제약.
   이게 없으면 32km 월드를 만든 의미가 이동 편의에 잠식된다.
3. **인스턴스 쿨다운의 개인 단위 적용.** OpenMMO 던전은 현재 공용 월드에 있다.
   인스턴스화를 도입한다면 쿨다운은 처음부터 개인 단위로 — 나중에 바꾸면
   기존 플레이어의 파밍 루프를 깨뜨린다.

**변형할 것**
- 맵 단위 규칙 → **존 단위 규칙**. RO가 맵에 붙이던 것(몬스터 정원, 레벨대,
  BGM, EXP 보정)을 OpenMMO는 `zones/` 사각형에 붙인다. 다만 현재 서버가 존 파일에서
  읽는 것은 `noSpawnZones`뿐이고 지상 스폰은 플레이어를 따라다니는
  `ambientSpawns`(`data-src/world.json`)다 — 존 기반 스포너는 **아직 없는 과제**이며
  `doc/TODO.md`의 "몬스터 스폰 개선 — 플레이어의 레벨에 맞게"와 같은 항목이다.
- 인스턴스 전체 사본 생성은 비용이 크다. OpenMMO 던전은 이미 **시드 결정적**이므로,
  "파티별 던전 인스턴스 = 파티 ID를 시드에 섞기"로 값싸게 흉내 낼 수 있다.

**기각할 것**
- 맵 전환 로딩 구조. 이음매 없는 월드가 이 프로젝트의 정체성이다
  ([doc/LOADING_OPTIMIZATION.md](../LOADING_OPTIMIZATION.md)).
- 도시별 전직 NPC. 전직 트리 자체를 채택하지 않는다 ([01_PROGRESSION](01_PROGRESSION.md)).

**성능 주의**
- 유료 순간이동은 **로딩 폭풍**을 만든다. 5,000명이 도시를 오갈 때 지형·오브젝트
  스트리밍이 동시에 몰리므로, 도착 지점을 소수의 고정 지점으로 제한하고
  타일 캐시 예열을 함께 설계한다.

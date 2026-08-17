# Ragnarok Online 시스템 레퍼런스 (iRO Wiki 기반)

OpenMMO에 RO식 시스템을 도입하기 위한 **설계 레퍼런스 묶음**. iRO Wiki에 정리된
Ragnarok Online의 시스템을 주제별로 재구성하고, 각 문서 끝에 **OpenMMO 현재 구현과의
대조 및 적용 판단**을 붙였다.

## 이 문서들의 성격

- 위키 페이지의 **복제본이 아니다.** iRO Wiki 본문은 제3자 저작물이므로 이 저장소에
  통째로 옮기지 않는다. 여기 있는 것은 시스템 구조·공식·수치를 확인해 **OpenMMO
  설계 언어로 다시 쓴 것**이고, 각 절에 원문 출처 링크를 남겼다.
- 수치는 **iRO Renewal 기준**이며, Classic이 다른 경우 따로 표기했다. 실제 구현 전에
  해당 출처 페이지에서 최신 값을 재확인할 것.
- RO를 그대로 복제하는 것이 목표가 아니다. OpenMMO는 D&D/NetHack 계열(능력치 3~18,
  4d6-drop-lowest)로 이미 방향이 잡혀 있고, 5,000 동접을 전제한 서버 권위 구조다.
  **무엇을 가져오고 무엇을 버리는지**가 이 묶음의 실질적인 산출물이며
  [09_OPENMMO_GAP_ANALYSIS](09_OPENMMO_GAP_ANALYSIS.md)에 모여 있다.

## 수집 방법과 한계 (중요)

이 문서는 **웹 검색 결과를 통해** 수집했다. 이 개발 환경의 egress 정책이
`irowiki.org` 직접 접속을 차단(CONNECT 403)하기 때문에 페이지 단위 크롤링을 하지
못했다. 그래서:

- **시스템 구조·공식·규칙 = 커버됨.** 아래 10개 문서가 다루는 범위.
- **대량 데이터베이스 = 미커버.** 개별 아이템/몬스터/스킬/맵 수천 건의 수치는
  `db.irowiki.org`(iW Database)에 있고, 이 환경에서는 받아올 수 없다. 실제로 필요할
  때는 egress 허용 목록에 `irowiki.org`, `db.irowiki.org`를 추가한 환경에서
  다시 수집해야 한다 — 절차는 [10_IMPLEMENTATION_ROADMAP](10_IMPLEMENTATION_ROADMAP.md) §6.
- 어차피 개별 아이템/몬스터 수치는 그대로 쓸 수 없다. OpenMMO의 `data-src/*.csv`
  스키마와 밸런스 축(능력치 3~18, 레벨 1~30급 곡선)이 RO(레벨 1~260)와 다르다.
  **가져와야 하는 것은 표가 아니라 구조다.**

## 문서 목록

| # | 문서 | 다루는 것 |
|---|------|-----------|
| 01 | [진행·성장](01_PROGRESSION.md) | Base/Job 레벨, EXP 곡선, 전직 트리, 스킬 포인트, 전생/3차/4차, 사망 페널티 |
| 02 | [스탯과 전투 공식](02_STATS_COMBAT.md) | 6스탯, ATK/MATK, HIT/FLEE, DEF/MDEF, ASPD, 치명타, 캐스팅, 데미지 파이프라인 |
| 03 | [속성·종족·상태이상](03_MODIFIERS.md) | 속성 상성, 크기/종족 배율, 상태이상 전체, 보스 프로토콜 |
| 04 | [아이템과 장비](04_ITEMS.md) | 장비 부위, 무기 레벨, 제련, 카드/슬롯, 인챈트, 무게/창고/카트 |
| 05 | [몬스터](05_MONSTERS.md) | 몬스터 속성, 선공/루터, MVP/미니보스, 리스폰, 드랍 |
| 06 | [월드와 던전](06_WORLD.md) | 도시/필드/던전 구조, 워프·세이브·카프라, 인스턴스(메모리얼 던전) |
| 07 | [사회·경제](07_SOCIAL_ECONOMY.md) | 파티, 길드, 공성전, 배틀그라운드/PvP, 제니 경제, 노점, 상인 스킬 |
| 08 | [콘텐츠 루프](08_CONTENT_LOOPS.md) | 퀘스트/일일/턴인, 제작(단조·조제·요리), 펫·호문쿨루스·용병 |
| 09 | [OpenMMO 갭 분석](09_OPENMMO_GAP_ANALYSIS.md) | 현재 구현 대조표, 채택/변형/기각 결정과 근거 |
| 10 | [구현 로드맵](10_IMPLEMENTATION_ROADMAP.md) | 단계별 계획, 데이터 스키마 변경안, 프로토콜 영향, 5,000 동접 제약, 데이터 수집 절차 |

## 읽는 순서

- 처음이면 09 → 10만 읽어도 개발 착수에는 충분하다.
- 특정 시스템을 구현하기 직전에 해당 번호 문서의 "OpenMMO 적용" 절을 읽는다.
- 구현 중 수치가 필요하면 각 절의 출처 링크로 원문을 확인한다 (추측하지 말 것).

## 주 출처

[iRO Wiki Main Page](https://irowiki.org/wiki/Main_Page) ·
[Stats](https://irowiki.org/wiki/Stats) ·
[Classes](https://irowiki.org/wiki/Classes) ·
[Skills](https://irowiki.org/wiki/Skills) ·
[Card System](https://irowiki.org/wiki/Card_System) ·
[Refinement System](https://irowiki.org/wiki/Refinement_System) ·
[Monster](https://irowiki.org/wiki/Monster) ·
[Places](https://irowiki.org/wiki/Places) ·
[Guild System](https://irowiki.org/wiki/Guild_System) ·
[Commerce](https://irowiki.org/wiki/Commerce) ·
[iW Database](https://db.irowiki.org/)

# 08. 콘텐츠 루프 (Quests, Crafting, Companions)

출처: [Quests](https://irowiki.org/wiki/Quests) · [Daily Quests](https://irowiki.org/wiki/Daily_Quests) ·
[Eden Group](https://irowiki.org/wiki/Eden_Group) ·
[Eden Group Leveling Quests](https://irowiki.org/wiki/Eden_Group_Leveling_Quests) ·
[Hunting Board Quests](https://irowiki.org/wiki/Hunting_Board_Quests) ·
[Bounty Board Quests](https://irowiki.org/wiki/Bounty_Board_Quests) ·
[Gramps Turn-In Monsters](https://irowiki.org/wiki/Gramps_Turn-In_Monsters) ·
[Forging](https://irowiki.org/wiki/Forging) · [Potion Creation](https://irowiki.org/wiki/Potion_Creation) ·
[Cooking](https://irowiki.org/wiki/Cooking) ·
[Cute Pet System](https://irowiki.org/wiki/Cute_Pet_System) ·
[Homunculus System](https://irowiki.org/wiki/Homunculus_System) ·
[Mercenary System](https://irowiki.org/wiki/Mercenary_System)

## 1. 퀘스트 계층

RO의 퀘스트는 **성격이 다른 세 층**으로 나뉜다.

| 층 | 예 | 성격 |
|----|-----|------|
| **일회성 서사/보상** | 전직 퀘스트, 장비 퀘스트 | 캐릭터당 1회. 해금이 보상 |
| **반복 사냥 퀘스트** | 에덴 그룹 레벨링 퀘스트, 헌팅 보드 | 무제한 반복. **EXP가 보상** |
| **일일/턴인** | Gramps 턴인, 데일리 퀘스트 | 하루 단위 제한. **효율이 압도적** |

- **에덴 그룹**: 게시판에서 원하는 만큼 수령, 대부분 즉시 반복 가능. 초보~중반
  레벨링의 **주 경로**이며 장비 퀘스트로 기본 장비까지 공급한다. 즉 에덴은
  "레벨 가이드 + 장비 공급"을 하나로 묶은 온보딩 장치다.
- **헌팅 보드**: 특정 몬스터 **150마리** 사냥 → EXP. 도시 주변 필드/던전 기준으로
  대상이 정해져 지역과 연결된다.
- **Gramps 턴인**: 저/중/고 구간별로 하루치 사냥 목표. 완료 시 **65,000z**(양쪽 완료
  130,000z) + 구간별 귀금속. RO 후반 레벨링과 제니 수급의 중심.
- 보상 티켓을 모아 교환하는 구조도 있다 (10 Courtesy Ticket → Old Blue Box /
  응축 백포션 60~100 / 이그드라실 씨앗 / 올드 카드 앨범 중 택1).

설계 요약: **일일 제한 콘텐츠가 가장 효율이 좋고, 반복 콘텐츠가 그 밑을 받친다.**
접속을 습관으로 만들되 하루 이상 몰아서 할 수는 없게 한다.

## 2. 제작

세 제작 모두 **성공률 공식**을 갖고, 스탯·직업 레벨·도구가 인자로 들어간다.

```
단조(Forging)   성공률 ← 장비 + 스킬 + DEX + LUK + Job Level
                DEX/LUK 1당 약 +0.1%, Job Level 1당 +0.2%
                속성석 −20%, 스타 크럼 1개당 −15%   (강한 옵션일수록 성공률을 판다)

조제(Brewing)   = (준비물약Lv × 3) + 물약연구Lv + 인스트럭션Lv
                  + (JobLv × 0.2) + (DEX × 0.1) + (LUK × 0.1) + (INT × 0.05) + 보정 %

요리(Cooking)   성공률 ← DEX, LUK, 레시피 난이도(↓), 요리 도구, 누적 요리 횟수(소폭 ↑)
```

공통 구조: **성공률 = 캐릭터 투자 + 도구 − 욕심**. 더 좋은 결과를 원할수록
성공률을 깎는 옵션을 스스로 붙인다. 제련([04_ITEMS](04_ITEMS.md) §3)과 같은 철학.

## 3. 동반자 (Companions)

| 시스템 | 유지 비용 | 성장 |
|--------|-----------|------|
| **펫 (Cute Pet)** | 먹이 — 친밀도 유지 | 친밀도 Cordial 이상에서 owner 보너스 |
| **호문쿨루스** (연금술사) | 먹이 — **허기 11~25 구간에서만** 친밀도 1 온전히 획득 | Loyal 도달 + 현자의 돌 → 진화 |
| **용병** | 없음. 대신 **계약 30분 후 자동 만료** | 함께 싸우면 Loyalty Point 축적 → 상위 용병 계약 가능 |

세 시스템의 대비가 교훈적이다. 펫/호문쿨루스는 **지속적 관리**를 요구하고
(먹이 타이밍을 놓치면 친밀도 하락), 용병은 관리 대신 **시간 제한**으로 비용을 낸다.
어느 쪽이든 "공짜 전력"은 없다.

## OpenMMO 적용

**현재 상태**
- 퀘스트 시스템이 **없다**. 이것이 현재 가장 큰 콘텐츠 공백이다.
- 반면 생활 콘텐츠는 있다: 낚시(스킬 레벨 0~30), 채집, 허기, 요리 계열
  (`grillsInto`, `nutrition`, `consumable`), 캠프파이어, 악기/공연(`musicPerformance`),
  팁 모자, 상인 딜.
- 제작(단조/조제)은 없고, 대신 **인챈트 주문서**가 도전형 강화를 담당한다.
- 동반자 시스템 없음. 단, **LLM으로 움직이는 NPC 에이전트**가 이미 있다 —
  RO의 어떤 동반자보다 야심찬 축이며 프로젝트의 정체성이다.

**가져올 것**
1. **헌팅 보드형 반복 퀘스트.** 퀘스트 시스템 전체를 만들지 않고도
   "특정 몬스터 N마리 → 보상"만으로 시작할 수 있다. 데이터는 CSV 한 장
   (`data-src/hunting_quests.csv`: 대상 몬스터, 수량, EXP/제니 보상, 레벨 구간),
   상태는 캐릭터당 진행 카운터. **가장 적은 구조로 방향성을 주는 콘텐츠**이며
   32km 월드에 "어디로 갈지"를 알려준다.
2. **일일 제한 + 고효율 구조.** 반복 퀘스트를 넣는 순간 무한 파밍이 되므로,
   일일 한도를 **처음부터** 설계에 넣는다. 나중에 붙이면 너프로 받아들여진다.
3. **성공률 = 투자 − 욕심.** 제작을 도입한다면 이 공식을 그대로 쓴다. OpenMMO의
   인챈트 사다리가 이미 같은 철학이라 플레이어가 규칙을 재학습할 필요가 없다.
4. **온보딩 퀘스트 라인(에덴 모델).** 레벨 가이드와 기본 장비 공급을 한 NPC에
   묶는 방식은 신규 유입 이탈을 막는 검증된 형태다. 도시 서비스 NPC
   ([06_WORLD](06_WORLD.md) §카프라)와 같은 자리에 둔다.

**변형할 것**
- 동반자: 펫/호문쿨루스 대신 **에이전트 NPC를 동반자로 계약**하는 형태가
  이 프로젝트답다. 유지 비용은 먹이가 아니라 **급여**(이미 `salaryPerDay`,
  `walletCap`이 `npcs.csv`에 있다) — 즉 기존 시스템의 확장으로 끝난다.
- 용병의 30분 계약 만료는 **LLM 호출 비용 통제**와 정확히 같은 문제다.
  계약 시간 제한은 재미 장치이자 비용 상한이 된다.

**기각할 것**
- 티켓 → 랜덤 박스 교환(올드 카드 앨범류). 카드 시스템을 채택하지 않고,
  랜덤 박스는 별도의 설계 판단(사행성)을 요구한다.
- 호문쿨루스식 세밀한 허기 관리(11~25 구간). OpenMMO는 이미 허기 시스템이 있고
  ([doc/HUNGER.md](../HUNGER.md)), 관리 대상을 하나 더 얹으면 피로가 겹친다.

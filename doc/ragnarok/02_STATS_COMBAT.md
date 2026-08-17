# 02. 스탯과 전투 공식 (Stats & Combat Formulas)

출처: [Stats](https://irowiki.org/wiki/Stats) · [ATK](https://irowiki.org/wiki/ATK) ·
[MATK](https://irowiki.org/wiki/MATK) · [FLEE](https://irowiki.org/wiki/FLEE) ·
[DEF](https://irowiki.org/wiki/DEF) · [MDEF](https://irowiki.org/wiki/MDEF) ·
[ASPD](https://irowiki.org/wiki/ASPD) · [Attacks](https://irowiki.org/wiki/Attacks) ·
[Skills](https://irowiki.org/wiki/Skills) ·
[Total stat point requirement](https://irowiki.org/wiki/Total_stat_point_requirement)

> Renewal 기준. 값 하나하나보다 **어떤 항이 어디에 곱해지고 더해지는지**가 설계의 본체다.

## 1. 6 능력치

STR / AGI / VIT / INT / DEX / LUK. 스탯 포인트로 올리며, **높을수록 비싸진다**.

| 구간 | 1 올리는 비용 | 누적 |
|------|---------------|------|
| 1 → 99 | `Floor[(X−1) ÷ 10] + 2` | 628 포인트 |
| 99 → 130 (3차) | `4 × Floor[(X−100) ÷ 5] + 16` | 787 포인트 |

체감 효과: 앞자리를 올릴수록 한계효용이 급감하므로 "주스탯 몰빵 vs 분산"이
실제 트레이드오프가 된다.

**Trait/Talent 스탯** (4차 도입): POW/STA/WIS/SPL/CON/CRT. 현재 수치와 무관하게
항상 1포인트, 상한 100. 후반 성장 축을 별도 통화로 분리한 설계.

## 2. 공격력

```
StatusATK (근접)   = (BaseLevel ÷ 4) + STR + (DEX ÷ 5) + (LUK ÷ 3)
StatusATK (원거리) = (BaseLevel ÷ 4) + (STR ÷ 5) + DEX + (LUK ÷ 3)
StatusMATK         = floor[ floor[BaseLv ÷ 4] + INT + floor[INT ÷ 2]
                            + floor[DEX ÷ 5] + floor[LUK ÷ 3] ]
```

무기 종류(활/총/악기/채찍)가 STR·DEX의 역할을 뒤집는다. **BaseLevel이 공격력에 직접
들어간다**는 점이 Renewal의 특징 — 레벨업 자체가 전투력이다.

## 3. 명중과 회피

```
HIT  = 175 + BaseLv + DEX + floor(LUK ÷ 3) + 2 × CON + 보너스
FLEE = 100 + BaseLv + AGI + floor(LUK ÷ 5) + 보너스     (표시상 A)
명중 확률 = (AttackerHIT − DefenderFLEE) %
```

- 스탯 창의 `A + B`에서 **B는 Perfect Dodge(럭키 회피)** — LUK 기반의 완전 회피
  확률이며 명중 계산과 별개로 굴린다.
- 명중이 뺄셈 한 방으로 끝나므로, 레벨 차가 그대로 적중률 차이가 된다.

## 4. 방어

**Hard DEF(비율) → Soft DEF(감산)** 순서로 적용한다.

```
Hard DEF 적용 후 = 데미지 × [ (4000 + HardDEF) ÷ (4000 + HardDEF × 10) ]
Hard MDEF 적용 후 = 데미지 × [ (1000 + HardMDEF) ÷ (1000 + HardMDEF × 10) ]
그 뒤 Soft DEF / Soft MDEF 를 절대값으로 차감
```

두 단계로 나눈 이유는 명확하다. 비율 감소만 있으면 고레벨에서 무한 방어가 되고,
감산만 있으면 소형 다단히트가 전부 0이 된다. **비율 먼저, 감산 나중**이 그 절충이다.

## 5. 공격속도 (ASPD)

```
Base ASPD = [ 200 − { 200 − ( 직업 기본 ASPD + 방패 페널티 − ASPD 보정
            + √(AGI × 9.999 + DEX × 0.19212) × ASPD 페널티 ) }
            × { 1 − 물약 ASPD − 스킬 ASPD } ]        (소수 2자리 내림)
Final ASPD = Base ASPD + 장비 ASPD% + 장비 ASPD 고정치
```

AGI가 제곱근으로 들어가므로 **초반 AGI 투자 효율이 압도적이고 후반에 급감**한다.
직업별 기본 ASPD와 무기/방패 페널티가 직업 정체성을 만든다.

## 6. 치명타

- 치명타는 **회피를 무시**하고 데미지를 `(40 + C.RATE)%` 만큼 올린다.
- `BaseCriticalMultiplier = 1.4` — 아무 보정 없는 치명타가 평타의 1.4배라는 뜻.
- 수면 상태의 대상은 **치명타 확률 2배**, 공격은 자동 명중.

## 7. 시전 시간과 딜레이

RO의 스킬은 **네 종류의 시간**을 구분한다. 이 구분이 전투 리듬 전체를 결정한다.

| 시간 | 의미 | 그 동안 가능한 것 |
|------|------|-------------------|
| **VCT** (가변 시전) | 스탯·장비로 줄어드는 시전 시간 | 이동 시 캔슬 |
| **FCT** (고정 시전) | 스킬 고유, 대부분 총 시전의 20% | 줄이기 매우 어려움 |
| **After-cast Delay** | 시전 후 전역 딜레이 | 이동·평타 O, **모든 스킬 X** |
| **Cooldown** | 해당 스킬 재사용 대기 | 이동·평타·**다른 스킬 O** |

```
VCT(초) = (BaseVCT − Sum_VCT) × (1 − √[(DEX × 2 + INT) ÷ 530])
                              × (1 − 장비감소%/100) × (1 − 스킬감소%/100)
FCT(초) = (BaseFCT − Sum_FCT) × (1 − 최대FCT감소%/100)
```

- **DEX 2배 + INT**가 √ 안에 들어간다 → 무캐스팅(instant cast) 임계점이 존재하고,
  그 지점을 맞추는 것이 캐릭터 빌딩의 목표가 된다.
- FCT 퍼센트 감소는 **중첩되지 않고**, 고정 감소는 중첩되며 퍼센트보다 먼저 적용.
- Cast Delay와 Cooldown은 **동시에 시작**한다.
- SP 등 자원 요구는 시전 시작이 아니라 **실행 단계에서 다시 검사**한다.

## 8. 데미지 파이프라인 요약

```
StatusATK/MATK + 무기ATK(+제련·변량)
  → 스킬 배율
  → 크기 배율 × 종족 배율 × 속성 상성        (03 문서)
  → 카드/장비 % 보정
  → Hard DEF(비율) → Soft DEF(감산)
  → 치명타 / 다단히트 분할
  → 최종 데미지 (최소 1 보장)
```

## OpenMMO 적용

**현재 상태** ([doc/COMBAT.md](../COMBAT.md))
- 능력치는 STR/DEX/CON/INT/WIS/CHA **3~18**, 4d6-drop-lowest + 클래스 보정 + 총합 72
  리밸런싱. 성장형 배분 없음.
- 전투 판정은 D&D식 주사위 기반, 전부 서버 처리. 몬스터는 `guard`(AC 유사),
  `damageRoll`(다이스 문자열), `attackBonus`를 가진다.
- 공격 타이밍은 애니메이션 기반: `attackCooldown` / `attackImpactDelay` /
  `attackDamageTextDelay` (ms).

**가져올 것**
1. **시간의 4분할.** 지금은 `attackCooldown` 하나뿐이다. 스킬을 도입하는 순간
   "시전 중 / 시전 후 전역 딜레이 / 개별 쿨다운"의 구분이 없으면 스킬 난사와
   애니메이션 깨짐이 동시에 터진다. RO의 4분할은 그 문제의 검증된 해답이고,
   OpenMMO의 애니메이션 파이프라인(`attackImpactDelay`)과도 잘 맞는다.
   → 스킬 시스템 설계 시 **가장 먼저 확정할 것**.
2. **비율 방어 → 감산 방어의 2단 구조.** D&D의 AC는 명중 판정만 바꾸고 데미지는
   줄이지 않는다. 방어구 성장 축을 넣으려면 감산만으로는 저레벨 다단 공격이
   0이 되므로, Hard/Soft 분리 아이디어가 유효하다.
3. **캐스팅 캔슬과 무캐스팅 임계점.** 스탯 투자에 명확한 목표점을 주는 장치.
   OpenMMO는 스탯이 3~18이라 √ 공식을 그대로 쓸 수 없고, 임계 구조만 가져온다.

**변형할 것**
- 모든 공식의 `BaseLevel` 항: OpenMMO도 레벨이 전투력에 직접 들어가는 편이
  레벨업 체감에 좋지만, 계수는 3~18 스탯 스케일에 맞춰 다시 잡아야 한다.
  예: `HIT = 10 + level + DEX modifier` 수준의 d20 친화적 스케일.
- 치명타 1.4배는 그대로 쓸 만한 값이다. D&D의 "2배 데미지"보다 완만해서 5,000명
  규모의 밸런싱에 덜 폭발적이다.

**기각할 것**
- ASPD 공식 전체. OpenMMO의 공격 주기는 애니메이션 클립 길이에 묶여 있고
  (`data-src/monsters.csv`, `player_anim_timing.csv`), 200 기준 역산 공식은
  애니메이션과 어긋난다. AGI형 속도 보정이 필요하면 **클립 재생 배속 + 쿨다운 배율**로
  구현한다.
- 스탯 포인트 비용 곡선 (628/787 포인트). 01 문서의 기각 사유와 동일.

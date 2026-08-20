import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, resolve } from 'node:path'

/**
 * The client mirrors two Rust enums by hand — `ClientMessage` in
 * `networkTypes.ts`, `SkillId` there and in `skillsStore.ts`. Nothing checked
 * that the copies agreed, and both drifted: twelve messages went missing and
 * left `npm run check` red on a merged commit, and `SkillId::Crafting` shipped
 * without a display name (IMP-5.2, gap analysis A).
 *
 * Type checking cannot catch this — a missing union member is only an error at
 * the call site, and a `Partial<Record<…>>` swallows a missing key entirely.
 * So the mirror is compared against the Rust source itself.
 */

const here = dirname(fileURLToPath(import.meta.url))
const repo = resolve(here, '../../../..')

const read = (path: string) => readFileSync(resolve(repo, path), 'utf-8')

/** Top-level variant names of a `pub enum`, ignoring nested braces. */
function rustVariants(source: string, enumName: string): string[] {
  const start = source.indexOf(`pub enum ${enumName} {`)
  expect(start, `enum ${enumName} not found`).toBeGreaterThan(-1)
  const body = source.slice(source.indexOf('{', start) + 1)

  const names: string[] = []
  let depth = 0
  for (const line of body.split('\n')) {
    const trimmed = line.trim()
    if (depth === 0) {
      if (trimmed === '}') break
      const match = /^([A-Z][A-Za-z0-9_]*)\s*(\{|,|$)/.exec(trimmed)
      if (match) names.push(match[1])
    }
    depth += (line.match(/\{/g)?.length ?? 0) - (line.match(/\}/g)?.length ?? 0)
    if (depth < 0) break
  }
  return names
}

/** Serde renames of a `pub enum` variant, i.e. the strings that go on the wire. */
function serdeNames(source: string, enumName: string): string[] {
  const start = source.indexOf(`pub enum ${enumName} {`)
  expect(start, `enum ${enumName} not found`).toBeGreaterThan(-1)
  const body = source.slice(start, source.indexOf('\n}', start))
  return [...body.matchAll(/#\[serde\(rename = "([a-z_]+)"\)\]/g)].map(
    (m) => m[1]
  )
}

/**
 * Messages the browser deliberately never sends. Each one is either
 * agent-only or refused by the server, and the reason is what keeps this list
 * from becoming a place to hide drift.
 */
const NOT_SENT_BY_THE_BROWSER: Record<string, string> = {
  // The NPC agent client's own handshake.
  AuthenticateNpc: 'agent-only',
  OfferDeal: 'agent-only — an NPC proposing a price',
  OpenTrade: 'agent-only — an NPC pushing its shop at a player',
  // Housing moved to the REST API; the server logs and ignores these.
  PlaceHouse: 'refused by the server — use the housing REST API',
  ModifyRoom: 'refused by the server — use the housing REST API',
  RemoveHouse: 'refused by the server — use the housing REST API',
}

describe('the client mirrors of the shared protocol', () => {
  it('covers every ClientMessage the browser can send', () => {
    const variants = rustVariants(
      read('shared/src/messages.rs'),
      'ClientMessage'
    )
    expect(variants.length).toBeGreaterThan(50)

    const union = read('client/src/lib/network/networkTypes.ts')
    const missing = variants
      .filter((name) => !(name in NOT_SENT_BY_THE_BROWSER))
      .filter((name) => !new RegExp(`\\b${name}\\b`).test(union))

    expect(
      missing,
      'add these to the ClientMessage union in networkTypes.ts, or to ' +
        'NOT_SENT_BY_THE_BROWSER with the reason'
    ).toEqual([])
  })

  it('keeps the exemption list honest', () => {
    const variants = rustVariants(
      read('shared/src/messages.rs'),
      'ClientMessage'
    )
    const stale = Object.keys(NOT_SENT_BY_THE_BROWSER).filter(
      (name) => !variants.includes(name)
    )
    expect(stale, 'these no longer exist in ClientMessage').toEqual([])
  })

  it('names every SkillId in the union and in the display map', () => {
    const skills = serdeNames(read('shared/src/skills.rs'), 'SkillId')
    expect(skills).toContain('fishing')

    const union = read('client/src/lib/network/networkTypes.ts')
    const unionBlock = /export type SkillId =([^\n]*(?:\n\s*\|[^\n]*)*)/.exec(
      union
    )?.[1]
    const combat = /export type CombatSkillId =([^\n]*)/.exec(union)?.[1] ?? ''
    const declared = `${unionBlock ?? ''}${combat}`

    const displayMap = read('client/src/lib/stores/skillsStore.ts')
    const names =
      /SKILL_DISPLAY_NAMES[^{]*\{([^}]*)\}/.exec(displayMap)?.[1] ?? ''

    for (const skill of skills) {
      expect(declared, `SkillId union is missing '${skill}'`).toContain(
        `'${skill}'`
      )
      expect(names, `SKILL_DISPLAY_NAMES is missing '${skill}'`).toContain(
        `${skill}:`
      )
    }
  })
})

import { describe, it, expect } from 'vitest'
import {
  channelPrefixOf,
  shouldBlockNpcTalkForPartyDraft,
  shouldRevertToSay,
  unescapeChannelPrefix,
} from './chatChannelStore'

describe('shouldRevertToSay', () => {
  it('reverts an empty draft when the party is gone', () => {
    expect(shouldRevertToSay(false, 'party', '')).toBe(true)
    expect(shouldRevertToSay(false, 'party', '   ')).toBe(true)
  })

  it('keeps a pending party draft off the public channel', () => {
    // The draft holds the party channel, so sending it draws the server's
    // private not-in-a-party refusal instead of local chat.
    expect(shouldRevertToSay(false, 'party', 'meet at the west gate')).toBe(
      false
    )
  })

  it('never reverts while the party is alive or already on say', () => {
    expect(shouldRevertToSay(true, 'party', '')).toBe(false)
    expect(shouldRevertToSay(false, 'say', '')).toBe(false)
  })
})

describe('shouldBlockNpcTalkForPartyDraft', () => {
  it('blocks retargeting a party draft to NPC talk', () => {
    expect(
      shouldBlockNpcTalkForPartyDraft('party', 'keep this in the party')
    ).toBe(true)
  })

  it('allows NPC talk when no party draft would be exposed', () => {
    expect(shouldBlockNpcTalkForPartyDraft('party', '   ')).toBe(false)
    expect(shouldBlockNpcTalkForPartyDraft('say', 'hello')).toBe(false)
  })
})

describe('channelPrefixOf', () => {
  it('reads the channel a line addresses', () => {
    expect(channelPrefixOf('%hi')).toBe('%')
    expect(channelPrefixOf('$hi')).toBe('$')
    expect(channelPrefixOf('hi')).toBe(null)
    expect(channelPrefixOf('')).toBe(null)
  })

  it('treats a doubled prefix as an escape, not an address', () => {
    expect(channelPrefixOf('%%hi')).toBe(null)
    expect(channelPrefixOf('$$hi')).toBe(null)
  })

  it('treats a bare prefix as plain text', () => {
    expect(channelPrefixOf('%')).toBe(null)
    expect(channelPrefixOf('$')).toBe(null)
  })
})

describe('unescapeChannelPrefix', () => {
  it('drops the escape and nothing else', () => {
    expect(unescapeChannelPrefix('%%50 off')).toBe('%50 off')
    expect(unescapeChannelPrefix('%hi')).toBe('%hi')
    expect(unescapeChannelPrefix('hi')).toBe('hi')
    expect(unescapeChannelPrefix('')).toBe('')
  })
})

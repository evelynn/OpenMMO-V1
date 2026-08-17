import { writable } from 'svelte/store'

/** Where plain input lines go: local say or the party channel. Sticky — a
 *  `/p` (or the input-bar toggle) holds until `/s` switches back; leaving
 *  the party reverts it (see ChatPanel's roster effect). */
export type ChatChannel = 'say' | 'party'

export const chatChannel = writable<ChatChannel>('say')

/** Losing the party reverts the input to say — but only once the draft is
 *  empty. A pending line keeps the party channel, so it can only fall into
 *  the server's private not-in-a-party refusal, never into public chat. */
export function shouldRevertToSay(
  inParty: boolean,
  channel: ChatChannel,
  draft: string
): boolean {
  return !inParty && channel === 'party' && draft.trim().length === 0
}

export function shouldBlockNpcTalkForPartyDraft(
  channel: ChatChannel,
  draft: string
): boolean {
  return channel === 'party' && draft.trim().length > 0
}

/** One-line channel prefixes (doc/ragnarok/12_UX_SERVICES.md §2). Kept as
 *  literals so the preview cannot address a channel the server does not. */
export const PARTY_CHAT_PREFIX = '%'
export const GUILD_CHAT_PREFIX = '$'
export type ChannelPrefix = typeof PARTY_CHAT_PREFIX | typeof GUILD_CHAT_PREFIX

/** The channel this line addresses, or null for ordinary text. Mirrors
 *  shared/src/messages.rs `split_channel_prefix`: a doubled prefix is an
 *  escape, and a bare prefix addresses nothing. The server re-parses and has
 *  the last word — this only drives the input-bar preview. */
export function channelPrefixOf(text: string): ChannelPrefix | null {
  const first = text[0]
  if (first !== PARTY_CHAT_PREFIX && first !== GUILD_CHAT_PREFIX) return null
  const rest = text.slice(1)
  if (rest === '' || rest.startsWith(first)) return null
  return first
}

/** Drop an escaped prefix's escape character. Needed only on the sticky
 *  party path, which bypasses the server's parser. */
export function unescapeChannelPrefix(text: string): string {
  const first = text[0]
  const isPrefix = first === PARTY_CHAT_PREFIX || first === GUILD_CHAT_PREFIX
  return isPrefix && text.slice(1).startsWith(first) ? text.slice(1) : text
}

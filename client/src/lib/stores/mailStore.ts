import { writable } from 'svelte/store'

/** One attachment on a piece of mail (ServerMessage::MailSummary.items). */
export interface MailAttachment {
  item_def_id: string
  quantity: number
  enchant: number
}

/** A mailbox row. Only ever arrives in reply to OpenMailbox — the steady-state
 *  push is the unread count alone. */
export interface MailEntry {
  id: number
  sender: string
  subject: string
  body: string
  gold: number
  items: MailAttachment[]
  created_at: number
  expires_at: number
  read: boolean
}

/** Mirrors the server-side `MAX_MAILBOX`; shown, not enforced, here. */
export const MAX_MAILBOX = 30

/** The letters, newest first. Empty until the panel is opened. */
export const mailList = writable<MailEntry[]>([])

/** Badge count pushed by the server. Independent of `mailList`, which is only
 *  filled while the panel is open. */
export const unreadMail = writable(0)

export const mailPanelVisible = writable(false)

/** True between sending OpenMailbox and the MailList reply. */
export const mailLoading = writable(false)

export function resetMailStores() {
  mailList.set([])
  unreadMail.set(0)
  mailPanelVisible.set(false)
  mailLoading.set(false)
}

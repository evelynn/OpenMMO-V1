/** Mirrors `shared/src/craft.rs` — the server is the authority; these keep
 *  the client from sending a request it already knows will be refused. */
export const MAX_CRAFT_OPTIONS = 3
export const CRAFT_BP_SCALE = 10_000

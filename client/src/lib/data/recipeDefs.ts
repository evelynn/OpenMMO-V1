/**
 * Crafting recipes, embedded at build time from data/recipes.json (generated
 * from data-src/recipes.csv). The server reads the same file, so the odds and
 * the materials never travel over the network (IMP-4.4).
 */
import recipesJson from '../../../../data/recipes.json'

export interface RecipeDef {
  id: string
  name: string
  output: string
  /** `item_def_id*qty`, semicolon-separated. */
  inputs: string
  baseSuccessBp: number
  skillId: string
  dexK?: number
  skillM?: number
  optionPenaltyBp?: number
  /** Copper an NPC charges to make it for you, never failing. */
  npcFee?: number
}

const recipeDefs = recipesJson as Record<string, RecipeDef>

export const RECIPES: RecipeDef[] = Object.values(recipeDefs)

export function getRecipe(id: string): RecipeDef | undefined {
  return recipeDefs[id]
}

/** Materials as `[item def id, quantity]` pairs, in row order. */
export function recipeMaterials(recipe: RecipeDef): [string, number][] {
  return recipe.inputs
    .split(';')
    .map((entry) => entry.trim())
    .filter(Boolean)
    .map((entry) => {
      const [id, qty] = entry.split('*')
      return [id.trim(), Number(qty ?? 1) || 1] as [string, number]
    })
}

export default recipeDefs

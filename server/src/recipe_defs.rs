use crate::item_defs::ItemDefs;
use onlinerpg_shared::skills::SkillId;
use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;
use tracing::info;

/// One crafting recipe (`data/recipes.json`, from `data-src/recipes.csv`).
/// Both branches — a commissioned NPC craft and a player's own attempt at a
/// fire — read the same row; only who rolls differs (IMP-4.4).
#[derive(Debug, Clone, Deserialize)]
pub struct RecipeDef {
    pub id: String,
    pub name: String,
    /// Item def id produced.
    pub output: String,
    /// `item_def_id*qty`, semicolon-separated.
    pub inputs: String,
    #[serde(rename = "baseSuccessBp")]
    pub base_success_bp: u32,
    #[serde(rename = "skillId")]
    pub skill_id: String,
    #[serde(rename = "dexK", default)]
    pub dex_k: u32,
    #[serde(rename = "skillM", default)]
    pub skill_m: u32,
    /// Success cost of aiming one enchant level higher.
    #[serde(rename = "optionPenaltyBp", default)]
    pub option_penalty_bp: u32,
    /// Copper an NPC charges to do it for you, never failing.
    #[serde(rename = "npcFee", default)]
    pub npc_fee: i64,
}

impl RecipeDef {
    /// Materials as (item def id, quantity) pairs, in row order.
    pub fn materials(&self) -> Vec<(String, u32)> {
        self.inputs
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|entry| match entry.split_once('*') {
                Some((id, qty)) => (
                    id.trim().to_string(),
                    qty.trim().parse().unwrap_or_else(|_| {
                        panic!("recipe '{}' has a non-numeric quantity", self.id)
                    }),
                ),
                None => (entry.to_string(), 1),
            })
            .collect()
    }

    pub fn skill(&self) -> SkillId {
        SkillId::from_str(&self.skill_id)
            .unwrap_or_else(|_| panic!("recipe '{}' names no known skill", self.id))
    }
}

#[derive(Debug, Clone)]
pub struct RecipeDefs {
    by_id: HashMap<String, RecipeDef>,
}

impl RecipeDefs {
    /// Load and validate against `item_defs`: an output or material that
    /// names no real item is a recipe nobody could ever complete, so it
    /// fails the boot rather than the craft.
    pub fn load(item_defs: &ItemDefs) -> Self {
        let data = include_str!("../../data/recipes.json");
        let by_id: HashMap<String, RecipeDef> =
            serde_json::from_str(data).expect("Failed to parse recipes.json");

        info!("Loaded {} crafting recipes", by_id.len());
        for recipe in by_id.values() {
            assert!(
                item_defs.get(&recipe.output).is_some(),
                "recipe '{}' produces unknown item '{}'",
                recipe.id,
                recipe.output
            );
            let materials = recipe.materials();
            assert!(
                !materials.is_empty(),
                "recipe '{}' has no inputs",
                recipe.id
            );
            for (id, qty) in &materials {
                assert!(
                    item_defs.get(id).is_some(),
                    "recipe '{}' needs unknown item '{}'",
                    recipe.id,
                    id
                );
                assert!(*qty > 0, "recipe '{}' asks for 0 of '{}'", recipe.id, id);
            }
            // Panics on an unknown skill id.
            let _ = recipe.skill();
            assert!(
                recipe.base_success_bp <= onlinerpg_shared::craft::CRAFT_BP_SCALE,
                "recipe '{}' has a base chance above certainty",
                recipe.id
            );
        }
        Self { by_id }
    }

    pub fn get(&self, id: &str) -> Option<&RecipeDef> {
        self.by_id.get(id)
    }
}

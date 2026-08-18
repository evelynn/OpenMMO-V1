//! Paid-travel destinations from data-src/travel_nodes.csv (IMP-2.4).
//!
//! Destinations are a fixed table, never arbitrary coordinates: opening
//! travel to any point would demote crossing a 32km world from content to a
//! menu. Dungeons are excluded at boot rather than at request time — a typo
//! in the csv is a data mistake, and failing to start is the only way to
//! catch it before a player does.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;

fn default_min_level() -> u32 {
    1
}

#[derive(Debug, Clone, Deserialize)]
pub struct TravelNode {
    pub id: String,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Copper, burned rather than paid to anyone.
    #[serde(default)]
    pub fare: i64,
    #[serde(rename = "minLevel", default = "default_min_level")]
    pub min_level: u32,
}

static NODES: LazyLock<Vec<TravelNode>> = LazyLock::new(|| {
    let by_id: HashMap<String, TravelNode> =
        serde_json::from_str(include_str!("../../data/travel_nodes.json"))
            .expect("Failed to parse travel_nodes.json");
    let mut nodes: Vec<TravelNode> = by_id.into_values().collect();
    nodes.sort_by(|a, b| a.fare.cmp(&b.fare).then_with(|| a.id.cmp(&b.id)));
    nodes
});

pub fn travel_nodes() -> &'static [TravelNode] {
    &NODES
}

pub fn travel_node(id: &str) -> Option<&'static TravelNode> {
    NODES.iter().find(|n| n.id == id)
}

/// Boot check: no destination may sit inside a dungeon's footprint, and the
/// list stays small on purpose — arrivals have to concentrate on a few spots
/// for the client's tile cache to be worth anything (SPK-3).
pub fn assert_nodes_are_valid() {
    if let Err(problem) = validate_nodes(travel_nodes()) {
        panic!("{problem}");
    }
    tracing::info!("Loaded {} travel destinations", travel_nodes().len());
}

/// Separated from the assert so the rules can be tested against data that
/// would otherwise have to ship broken.
fn validate_nodes(nodes: &[TravelNode]) -> Result<(), String> {
    if nodes.len() > MAX_TRAVEL_NODES {
        return Err(format!(
            "travel_nodes.csv has {} destinations; keep it to {MAX_TRAVEL_NODES} or fewer so \
             arrivals concentrate (SPK-3)",
            nodes.len()
        ));
    }
    for node in nodes {
        if let Some(entrance) = nearest_mouth(node.x, node.z) {
            return Err(format!(
                "travel node '{}' is at the mouth of '{}' - dungeons are walked to",
                node.id, entrance
            ));
        }
        if node.fare < 0 {
            return Err(format!("travel node '{}' has a negative fare", node.id));
        }
    }
    Ok(())
}

/// Ten is the ceiling doc 13 sets, for the loading reason above.
const MAX_TRAVEL_NODES: usize = 10;

/// How close to a dungeon entrance counts as arriving at it. Deliberately not
/// the 80x80 dungeon footprint: the starting town sits inside old_crypt's,
/// 22-37m from the mouth, so the footprint would rule out the one destination
/// the service exists to offer (doc 13 IMP-2.4 revision).
const DUNGEON_MOUTH_RADIUS: f32 = 20.0;

/// The dungeon whose mouth this point stands at, if any.
fn nearest_mouth(x: f32, z: f32) -> Option<&'static str> {
    onlinerpg_shared::dungeon::entrances()
        .iter()
        .find(|d| {
            let (dx, dz) = (d.x - x, d.z - z);
            dx * dx + dz * dz <= DUNGEON_MOUTH_RADIUS * DUNGEON_MOUTH_RADIUS
        })
        .map(|d| d.id.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, x: f32, z: f32, fare: i64) -> TravelNode {
        TravelNode {
            id: id.to_string(),
            name: id.to_string(),
            x,
            y: 0.0,
            z,
            fare,
            min_level: 1,
        }
    }

    #[test]
    fn the_shipped_table_is_valid() {
        validate_nodes(travel_nodes()).expect("travel_nodes.csv");
        assert!(
            !travel_nodes().is_empty(),
            "a travel service with nowhere to go is not a service"
        );
    }

    /// The rule that matters: a destination at a dungeon mouth would turn the
    /// walk into a menu, and the boot is where a csv typo gets caught.
    #[test]
    fn a_destination_at_a_dungeon_mouth_is_refused() {
        let crypt = onlinerpg_shared::dungeon::entrances()
            .first()
            .expect("at least one dungeon");
        let bad = [node("cheat", crypt.x, crypt.z, 0)];
        let problem = validate_nodes(&bad).expect_err("must be refused");
        assert!(problem.contains("mouth"), "{problem}");
    }

    /// The town is built over the old crypt, well inside its 80x80 footprint.
    /// A footprint test would refuse the capital, which is the destination
    /// the whole service is for.
    #[test]
    fn the_starting_town_is_a_legal_destination() {
        let spawn = &crate::world_config::world_config().spawn_position;
        assert!(
            onlinerpg_shared::dungeon::entrance_at(spawn.x, spawn.z).is_some(),
            "this test is only meaningful while the town sits in a footprint"
        );
        validate_nodes(&[node("capital", spawn.x, spawn.z, 0)])
            .expect("the starting town must remain reachable");
    }

    #[test]
    fn the_table_stays_short_enough_for_arrivals_to_concentrate() {
        let many: Vec<TravelNode> = (0..=MAX_TRAVEL_NODES)
            .map(|i| node(&format!("n{i}"), 0.0, 0.0, 0))
            .collect();
        assert!(validate_nodes(&many).is_err());
    }

    #[test]
    fn a_negative_fare_is_refused() {
        assert!(validate_nodes(&[node("free_money", 0.0, 0.0, -100)]).is_err());
    }
}

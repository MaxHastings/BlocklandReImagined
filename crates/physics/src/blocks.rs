//! Collision groups for what a collider blocks ([`bri_content::blocks`]).
//!
//! A map collider marked with the query types it blocks gets those query types' bits as its only memberships; a query
//! asks for its own bit plus every bit that is not a query-type bit, so unmasked colliders (all memberships) and
//! everything that is not a map collision mesh still answer to it, and a masked collider answers only when it lists
//! the query. A query that asks for no kind (the groups every older caller passes) still meets every collider.
use bri_content::blocks;
use rapier3d::prelude::*;

/// The group bit of each query type (clear of the corpse group and the pass groups).
pub const MOVE: Group = Group::GROUP_11;
pub const RAY: Group = Group::GROUP_12;
pub const SWEEP: Group = Group::GROUP_13;
pub const SHOT: Group = Group::GROUP_14;
/// All the query-type bits.
pub const KINDS: Group =
    Group::from_bits_truncate(MOVE.bits() | RAY.bits() | SWEEP.bits() | SHOT.bits());

/// The group bit of a query-type name ([`bri_content::blocks`]).
pub fn bit(name: &str) -> Option<Group> {
    match name {
        blocks::MOVE => Some(MOVE),
        blocks::RAY => Some(RAY),
        blocks::SWEEP => Some(SWEEP),
        blocks::SHOT => Some(SHOT),
        _ => None,
    }
}

/// The groups of a collider that blocks the named query types; `None` blocks every one (the default groups).
pub fn collider_groups(blocks: Option<&[String]>) -> InteractionGroups {
    let Some(blocks) = blocks else {
        return InteractionGroups::all();
    };
    let memberships = blocks
        .iter()
        .filter_map(|n| bit(n))
        .fold(Group::NONE, |all, g| all | g);
    InteractionGroups::new(memberships, Group::ALL, InteractionTestMode::And)
}

/// What a query of one type collides with: everything that is not a query-type bit, and colliders that list this type.
pub const fn query_filter_bits(kind: Group) -> Group {
    Group::ALL.difference(KINDS).union(kind)
}

/// The groups a query of one type passes to a query filter: it meets every collider that is not a masked map collider
/// and the masked ones that list its type.
pub const fn query_groups(kind: Group) -> InteractionGroups {
    InteractionGroups::new(
        Group::ALL,
        query_filter_bits(kind),
        InteractionTestMode::And,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meets(collider: InteractionGroups, query: InteractionGroups) -> bool {
        collider.test(query)
    }

    #[test]
    fn a_masked_collider_answers_only_the_queries_it_lists() {
        let wall = collider_groups(Some(&["move".to_string(), "sweep".to_string()]));
        let query =
            |k| InteractionGroups::new(Group::ALL, query_filter_bits(k), InteractionTestMode::And);
        assert!(meets(wall, query(MOVE)));
        assert!(meets(wall, query(SWEEP)));
        assert!(!meets(wall, query(RAY)));
        assert!(!meets(wall, query(SHOT)));
    }

    #[test]
    fn an_unmasked_collider_and_an_old_query_meet_everything() {
        let any = collider_groups(None);
        let masked = collider_groups(Some(&["shot".to_string()]));
        for k in [MOVE, RAY, SWEEP, SHOT] {
            assert!(meets(
                any,
                InteractionGroups::new(Group::ALL, query_filter_bits(k), InteractionTestMode::And)
            ));
        }
        assert!(meets(masked, InteractionGroups::all()));
    }
}

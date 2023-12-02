use std::{collections::HashMap, ops::Deref};

use emerald::{serde::Deserialize, Emerald, EmeraldError, Entity, Transform, World};

struct Initted {}
pub fn init(emd: &mut Emerald) {
    if emd.resources().contains::<Initted>() {
        return;
    }
    println!("init");
    emd.loader().add_on_world_load_hook(on_world_load);
    emd.loader().add_world_merge_handler(on_world_merge);
    emd.loader().register_component::<TempId>("parent_id");
    emd.loader().register_component::<TempParent>("parent");
    emd.resources().insert(Initted {});
}

fn on_world_load(world: &mut World) -> Result<(), EmeraldError> {
    let all_temp_parents = world.collect_by::<TempParent>();
    all_temp_parents.into_iter().for_each(|id| {
        let temp_parent = world.remove_one::<TempParent>(id).unwrap();
        let parent = world
            .query::<&TempId>()
            .iter()
            .find(|(_, i)| &i.name == &temp_parent.parent)
            .map(|(id, _)| id);
        parent.map(|parent_id| {
            world
                .insert_one(
                    id,
                    Parent {
                        entity: parent_id,
                        offset: temp_parent.offset,
                    },
                )
                .ok();
        });
    });

    let all_temp_ids = world.collect_by::<TempId>();
    all_temp_ids.into_iter().for_each(|id| {
        world.remove_one::<TempId>(id).ok();
    });

    Ok(())
}

fn on_world_merge(
    new_world: &mut World,
    _old_world: &mut World,
    entity_map: &mut HashMap<Entity, Entity>,
) -> Result<(), EmeraldError> {
    for (old_entity, new_entity) in entity_map.iter() {
        new_world
            .get::<&mut Parent>(new_entity.clone())
            .ok()
            .map(|mut p| {
                entity_map.get(&p.entity).map(|e| {
                    p.entity = e.clone();
                });
            });
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(crate = "emerald::serde")]
struct TempParent {
    parent: String,

    #[serde(default)]
    offset: Transform,
}

#[derive(Deserialize)]
#[serde(crate = "emerald::serde")]
struct TempId {
    name: String,
}

pub struct Parent {
    pub entity: Entity,
    pub offset: Transform,
}

/// Updates the positions of all children to be relative to their parents
pub fn hierarchy_system(world: &mut World) {
    // Construct a view for efficient random access into the set of all entities that have
    // parents. Views allow work like dynamic borrow checking or component storage look-up to be
    // done once rather than per-entity as in `World::get`.
    let mut parents = world.query::<&Parent>();
    let parents = parents.view();

    // View of entities that don't have parents, i.e. roots of the transform hierarchy
    let mut roots = world.query::<&Transform>().without::<&Parent>();
    let roots = roots.view();

    // This query can coexist with the `roots` view without illegal aliasing of `Transform`
    // references because the inclusion of `&Parent` in the query, and its exclusion from the view,
    // guarantees that they will never overlap. Similarly, it can coexist with `parents` because
    // that view does not reference `Transform`s at all.
    for (_entity, (parent, absolute)) in world.query::<(&Parent, &mut Transform)>().iter() {
        println!("parenting");
        // Walk the hierarchy from this entity to the root, accumulating the entity's absolute
        // transform. This does a small amount of redundant work for intermediate levels of deeper
        // hierarchies, but unlike a top-down traversal, avoids tracking entity child lists and is
        // cache-friendly.
        let mut relative = parent.offset;
        let mut ancestor = parent.entity;
        while let Some(next) = parents.get(ancestor) {
            relative = next.offset + relative;
            ancestor = next.entity;
        }
        // The `while` loop terminates when `ancestor` cannot be found in `parents`, i.e. when it
        // does not have a `Parent` component, and is therefore necessarily a root.
        roots.get(ancestor).map(|t| {
            *absolute = *t + relative;
        });
    }
}

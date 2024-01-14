use std::collections::HashMap;

use emerald::{
    serde::Deserialize, Emerald, EmeraldError, Entity, OnWorldLoadContext, Transform, World,
    WorldMerge,
};

struct Initted {}
pub fn init(emd: &mut Emerald) {
    if emd.resources().contains::<Initted>() {
        return;
    }
    emd.loader().add_on_world_load_hook(on_world_load);
    emd.loader().add_world_merge_handler(on_world_merge);
    emd.loader().register_component::<TempId>("parent_id");
    emd.loader().register_component::<TempParent>("parent");
    emd.resources().insert(Initted {});
}

struct OnParentHooks {
    hooks: HashMap<usize, OnParentHook>,
    uid: usize,
}

pub fn add_on_parented_hook(emd: &mut Emerald, hook: OnParentHook) {
    if !emd.resources().contains::<OnParentHooks>() {
        emd.resources().insert(OnParentHooks {
            hooks: HashMap::new(),
            uid: 0,
        });
    }

    emd.resources().get_mut::<OnParentHooks>().map(|h| {
        h.uid += 1;
        h.hooks.insert(h.uid, hook);
    });
}

fn on_world_load(ctx: OnWorldLoadContext, world: &mut World) -> Result<(), EmeraldError> {
    let all_temp_parents = world.collect_by::<TempParent>();
    let mut parented = Vec::new();
    all_temp_parents.into_iter().for_each(|id| {
        let temp_parent = world.remove_one::<TempParent>(id).unwrap();
        let parent = world
            .query::<&TempId>()
            .iter()
            .find(|(_, i)| &i.name == &temp_parent.parent)
            .map(|(id, _)| id);
        parent.map(|parent_id| {
            parented.push(OnParentHookContext {
                parent: parent_id,
                child: id,
            });
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

    ctx.resources.get::<OnParentHooks>().map(|h| {
        parented.into_iter().for_each(|p_ctx| {
            for (_, hook) in &h.hooks {
                (hook)(world, &p_ctx);
            }
        });
    });
    Ok(())
}

fn on_world_merge(
    new_world: &mut World,
    _old_world: &mut World,
    entity_map: &mut HashMap<Entity, Entity>,
    ctx: &WorldMerge,
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

pub struct OnParentHookContext {
    pub parent: Entity,
    pub child: Entity,
}
pub type OnParentHook = fn(world: &mut World, ctx: &OnParentHookContext);

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

pub fn get_entity_by_temp_parent_id(world: &World, id: &str) -> Option<Entity> {
    world
        .query::<&TempId>()
        .iter()
        .find(|(e, i)| &i.name == id)
        .map(|(e, _)| e.clone())
}

pub fn get_children(world: &World, parent_id: Entity) -> Vec<Entity> {
    world
        .query::<&Parent>()
        .iter()
        .filter_map(|(id, p)| (p.entity == parent_id).then(|| id))
        .collect()
}

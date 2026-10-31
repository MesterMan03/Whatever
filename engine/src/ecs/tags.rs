use super::{EntityId, World};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_ENTITY_TAGS: usize = 32;

pub fn validate_tag(tag: &str) -> Result<()> {
    ensure!(
        !tag.trim().is_empty() && tag.len() <= 128 && !tag.chars().any(char::is_control),
        "tags must contain 1–128 UTF-8 bytes and no control characters"
    );
    Ok(())
}

pub fn validate_tags(tags: &[String]) -> Result<()> {
    ensure!(
        tags.len() <= MAX_ENTITY_TAGS,
        "an entity can have at most 32 tags"
    );
    let mut unique = HashSet::new();
    for tag in tags {
        validate_tag(tag)?;
        ensure!(unique.insert(tag), "duplicate tag '{tag}'");
    }
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TagQuery {
    #[serde(default)]
    pub all: Vec<String>,
    #[serde(default)]
    pub any: Vec<String>,
    #[serde(default)]
    pub none: Vec<String>,
    #[serde(default)]
    pub include_disabled: bool,
}

impl World {
    pub fn set_tags(&mut self, id: &EntityId, tags: Vec<String>) -> Result<()> {
        ensure!(self.is_alive(id), "entity no longer exists");
        validate_tags(&tags)?;
        self.mark_metadata(*id);
        if tags.is_empty() {
            self.tags.remove(&id.index);
        } else {
            self.tags.insert(id.index, tags);
        }
        Ok(())
    }

    pub fn add_tag(&mut self, id: &EntityId, tag: String) -> Result<()> {
        ensure!(self.is_alive(id), "entity no longer exists");
        validate_tag(&tag)?;
        self.mark_metadata(*id);
        let tags = self.tags.entry(id.index).or_default();
        if !tags.contains(&tag) {
            ensure!(
                tags.len() < MAX_ENTITY_TAGS,
                "an entity can have at most 32 tags"
            );
            tags.push(tag);
        }
        Ok(())
    }

    pub fn remove_tag(&mut self, id: &EntityId, tag: &str) -> Result<()> {
        ensure!(self.is_alive(id), "entity no longer exists");
        validate_tag(tag)?;
        self.mark_metadata(*id);
        if let Some(tags) = self.tags.get_mut(&id.index) {
            tags.retain(|existing| existing != tag);
        }
        Ok(())
    }

    pub fn entity_tags(&self, id: &EntityId) -> Result<Vec<String>> {
        ensure!(self.is_alive(id), "entity no longer exists");
        Ok(self.tags.get(&id.index).cloned().unwrap_or_default())
    }

    pub fn find_by_tags(&self, query: &TagQuery) -> Result<Vec<EntityId>> {
        for tags in [&query.all, &query.any, &query.none] {
            validate_tags(tags)?;
        }
        Ok(self
            .allocator
            .alive_entity_ids()
            .filter(|id| {
                if !query.include_disabled && !self.is_active(id) {
                    return false;
                }
                let tags = self.tags.get(&id.index).map(Vec::as_slice).unwrap_or(&[]);
                query.all.iter().all(|tag| tags.contains(tag))
                    && (query.any.is_empty() || query.any.iter().any(|tag| tags.contains(tag)))
                    && query.none.iter().all(|tag| !tags.contains(tag))
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_bounded_unique_and_follow_entity_lifetime() {
        let mut world = World::new();
        let entity = world.create_entity();
        for index in 0..32 {
            world.add_tag(&entity, format!("tag{index}")).expect("add");
        }
        world
            .add_tag(&entity, "tag0".into())
            .expect("duplicate is idempotent");
        assert!(world.add_tag(&entity, "overflow".into()).is_err());
        assert_eq!(world.entity_tags(&entity).expect("tags").len(), 32);
        let copy = world.clone_entity(entity, false, None).expect("clone");
        assert_eq!(
            world.entity_tags(&copy).expect("copy"),
            world.entity_tags(&entity).expect("original")
        );
        world.destroy_entity(entity);
        let reused = world.create_entity();
        assert!(world.entity_tags(&reused).expect("new").is_empty());
        assert!(world.add_tag(&entity, "stale".into()).is_err());
    }

    #[test]
    fn queries_combine_all_any_and_none_and_respect_disabled_ancestors() {
        let mut world = World::new();
        let parent = world.create_entity();
        let entity = world.create_entity();
        world.set_parent(entity, Some(parent));
        world
            .set_tags(&entity, vec!["enemy".into(), "flying".into()])
            .expect("tags");
        let mut query = TagQuery {
            all: vec!["enemy".into()],
            any: vec!["flying".into(), "swimming".into()],
            none: vec!["dead".into()],
            ..Default::default()
        };
        assert_eq!(world.find_by_tags(&query).expect("query"), vec![entity]);
        world.set_enabled(&parent, false);
        assert!(world.find_by_tags(&query).expect("disabled").is_empty());
        query.include_disabled = true;
        assert_eq!(world.find_by_tags(&query).expect("include"), vec![entity]);
        query.none.push("flying".into());
        assert!(world.find_by_tags(&query).expect("excluded").is_empty());
        assert!(
            world
                .set_tags(&entity, vec!["duplicate".into(); 2])
                .is_err()
        );
    }
}

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId {
    pub index: u32,
    pub generation: u32,
}

impl EntityId {
    pub fn parse(s: &str) -> Option<Self> {
        let (idx, gen_str) = s.split_once(':')?;
        Some(EntityId {
            index: idx.parse().ok()?,
            generation: gen_str.parse().ok()?,
        })
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.index, self.generation)
    }
}

struct EntitySlot {
    generation: u32,
    alive: bool,
}

pub struct EntityAllocator {
    slots: Vec<EntitySlot>,
    free: Vec<u32>,
}

impl EntityAllocator {
    pub fn new() -> Self {
        EntityAllocator {
            slots: Vec::new(),
            free: Vec::new(),
        }
    }

    pub fn alloc(&mut self) -> EntityId {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.alive = true;
            EntityId {
                index,
                generation: slot.generation,
            }
        } else {
            let index = self.slots.len() as u32;
            self.slots.push(EntitySlot {
                generation: 0,
                alive: true,
            });
            EntityId {
                index,
                generation: 0,
            }
        }
    }

    /// Returns false if `id` is stale (already freed or wrong generation).
    pub fn free(&mut self, id: EntityId) -> bool {
        let Some(slot) = self.slots.get_mut(id.index as usize) else {
            return false;
        };
        if !slot.alive || slot.generation != id.generation {
            return false;
        }
        slot.alive = false;
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(id.index);
        true
    }

    pub fn is_alive(&self, id: &EntityId) -> bool {
        self.slots
            .get(id.index as usize)
            .map(|s| s.alive && s.generation == id.generation)
            .unwrap_or(false)
    }

    pub fn alive_entity_ids(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.alive)
            .map(|(i, s)| EntityId {
                index: i as u32,
                generation: s.generation,
            })
    }
}

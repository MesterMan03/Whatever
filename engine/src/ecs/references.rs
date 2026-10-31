use super::EntityId;
use anyhow::{Context, ensure};
use serde_json::Value;

pub fn serialize_reference<S: serde::Serializer>(
    id: &EntityId,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(Some(1))?;
    map.serialize_entry("$entity", &id.to_string())?;
    map.end()
}

/// Resolve only explicitly tagged references. Ordinary strings remain ordinary data.
pub fn resolve_references(
    value: &mut Value,
    resolve: &mut impl FnMut(&str) -> Option<EntityId>,
) -> anyhow::Result<()> {
    match value {
        Value::Object(fields) if fields.contains_key("$entity") => {
            ensure!(
                fields.len() == 1,
                "entity reference must contain only $entity"
            );
            let reference = fields["$entity"]
                .as_str()
                .context("$entity must be a string")?;
            let id = resolve(reference)
                .with_context(|| format!("unresolved entity reference '{reference}'"))?;
            fields.insert("$entity".into(), Value::String(id.to_string()));
        }
        Value::Object(fields) => {
            for item in fields.values_mut() {
                resolve_references(item, resolve)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                resolve_references(item, resolve)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn referenced_entity(value: &Value) -> Option<EntityId> {
    value
        .as_object()
        .filter(|fields| fields.len() == 1)?
        .get("$entity")?
        .as_str()
        .and_then(EntityId::parse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn nested_references_are_resolved_without_rewriting_strings() {
        let mut data = json!({"target":{"$entity":"pending:1"},"nested":[{"$entity":"3:7"}],"text":"pending:1"});
        resolve_references(&mut data, &mut |text| {
            if text == "pending:1" {
                Some(EntityId {
                    index: 2,
                    generation: 4,
                })
            } else {
                EntityId::parse(text)
            }
        })
        .expect("resolve");
        assert_eq!(
            data,
            json!({"target":{"$entity":"2:4"},"nested":[{"$entity":"3:7"}],"text":"pending:1"})
        );
        assert!(
            resolve_references(&mut json!({"$entity":"pending:unknown"}), &mut |_| None).is_err()
        );
        assert!(
            resolve_references(&mut json!({"$entity":"3:7","extra":true}), &mut |_| None).is_err()
        );
    }
}

//! Input-schema validation for catalog tool descriptors.
//!
//! Split from `lib.rs` under the module-size law (grandfathered modules may
//! only shrink; ADR-0049). The subset accepted here is deliberately closed:
//! only the JSON Schema keywords `ToolDesc::validate_arguments` can enforce
//! at dispatch are allowed to appear in a descriptor, so a schema can never
//! advertise a constraint the runtime does not check.

use std::collections::BTreeSet;

use serde_json::Value;

pub(crate) fn validate_input_schema(schema: &Value) -> Result<(), String> {
    let object = schema
        .as_object()
        .ok_or_else(|| "schema must be an object".to_string())?;
    for keyword in object.keys() {
        if !matches!(
            keyword.as_str(),
            "type" | "properties" | "required" | "additionalProperties"
        ) {
            return Err(format!("unsupported top-level keyword {keyword}"));
        }
    }
    if object.get("type").and_then(Value::as_str) != Some("object") {
        return Err("top-level type must be object".into());
    }
    let properties = object
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| "properties must be an object".to_string())?;
    let required = object
        .get("required")
        .and_then(Value::as_array)
        .ok_or_else(|| "required must be an array".to_string())?;
    if !object
        .get("additionalProperties")
        .is_some_and(Value::is_boolean)
    {
        return Err("additionalProperties must be boolean".into());
    }

    let mut required_names = BTreeSet::new();
    for value in required {
        let name = value
            .as_str()
            .ok_or_else(|| "required entries must be strings".to_string())?;
        if !properties.contains_key(name) {
            return Err(format!("required property {name} is not declared"));
        }
        if !required_names.insert(name) {
            return Err(format!("required property {name} is duplicated"));
        }
    }

    for (name, schema) in properties {
        let property = schema
            .as_object()
            .ok_or_else(|| format!("property {name} schema must be an object"))?;
        for keyword in property.keys() {
            if !matches!(keyword.as_str(), "type" | "enum" | "minimum" | "items") {
                return Err(format!("property {name} has unsupported keyword {keyword}"));
            }
        }
        let property_type = property
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("property {name} must declare a string type"))?;
        if !matches!(
            property_type,
            "string" | "integer" | "number" | "boolean" | "array" | "object"
        ) {
            return Err(format!(
                "property {name} has unsupported type {property_type}"
            ));
        }
        if let Some(values) = property.get("enum") {
            let values = values
                .as_array()
                .ok_or_else(|| format!("property {name} enum must be an array"))?;
            if values
                .iter()
                .any(|value| !value_matches_type(value, property_type))
            {
                return Err(format!("property {name} enum value has wrong type"));
            }
        }
        if let Some(minimum) = property.get("minimum") {
            if property_type != "integer" || minimum.as_i64().is_none() {
                return Err(format!(
                    "property {name} minimum requires an integer type and value"
                ));
            }
        }
        match property.get("items") {
            Some(items) if property_type == "array" => {
                let items = items
                    .as_object()
                    .ok_or_else(|| format!("property {name} items must be an object"))?;
                if items.len() != 1 || items.get("type").and_then(Value::as_str) != Some("string") {
                    return Err(format!("property {name} supports only string array items"));
                }
            }
            Some(_) => return Err(format!("property {name} items requires array type")),
            None if property_type == "array" => {
                return Err(format!("property {name} array must declare string items"));
            }
            None => {}
        }
    }
    Ok(())
}

fn value_matches_type(value: &Value, expected: &str) -> bool {
    match expected {
        "string" => value.is_string(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema(properties: Value, required: Value, additional_properties: bool) -> Value {
        json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": additional_properties,
        })
    }

    #[test]
    fn accepts_a_closed_valid_schema() {
        let input = schema(
            json!({
                "path": { "type": "string" },
                "limit": { "type": "integer", "minimum": 1 },
                "mode": { "type": "string", "enum": ["fast", "safe"] },
                "tags": { "type": "array", "items": { "type": "string" } },
                "enabled": { "type": "boolean" },
                "meta": { "type": "object" },
                "ratio": { "type": "number" },
            }),
            json!(["path"]),
            false,
        );
        assert!(validate_input_schema(&input).is_ok());
    }

    #[test]
    fn rejects_a_non_object_schema() {
        assert!(validate_input_schema(&json!("string")).is_err());
    }

    #[test]
    fn rejects_an_unsupported_top_level_keyword() {
        // A keyword the runtime cannot enforce must not be advertised.
        let input = json!({
            "type": "object",
            "properties": { "a": { "type": "string" } },
            "required": [],
            "additionalProperties": false,
            "title": "extra",
        });
        let err = validate_input_schema(&input).unwrap_err();
        assert!(err.contains("unsupported top-level keyword"));
    }

    #[test]
    fn rejects_required_that_is_not_declared() {
        let input = schema(json!({ "a": { "type": "string" } }), json!(["b"]), false);
        let err = validate_input_schema(&input).unwrap_err();
        assert!(err.contains("b") && err.contains("not declared"));
    }

    #[test]
    fn rejects_a_duplicated_required_entry() {
        let input = schema(
            json!({ "a": { "type": "string" } }),
            json!(["a", "a"]),
            false,
        );
        let err = validate_input_schema(&input).unwrap_err();
        assert!(err.contains("duplicated"));
    }

    #[test]
    fn rejects_an_enum_value_of_the_wrong_type() {
        let input = schema(
            json!({ "mode": { "type": "string", "enum": ["a", 3] } }),
            json!([]),
            false,
        );
        let err = validate_input_schema(&input).unwrap_err();
        assert!(err.contains("enum value has wrong type"));
    }

    #[test]
    fn rejects_minimum_on_a_non_integer_property() {
        let input = schema(
            json!({ "ratio": { "type": "number", "minimum": 0 } }),
            json!([]),
            false,
        );
        let err = validate_input_schema(&input).unwrap_err();
        assert!(err.contains("minimum"));
    }

    #[test]
    fn rejects_non_string_array_items() {
        let input = schema(
            json!({ "tags": { "type": "array", "items": { "type": "integer" } } }),
            json!([]),
            false,
        );
        let err = validate_input_schema(&input).unwrap_err();
        assert!(err.contains("only string array items"));
    }

    #[test]
    fn rejects_items_on_a_non_array_property() {
        let input = schema(
            json!({ "name": { "type": "string", "items": { "type": "string" } } }),
            json!([]),
            false,
        );
        let err = validate_input_schema(&input).unwrap_err();
        assert!(err.contains("items requires array type"));
    }

    #[test]
    fn rejects_an_unsupported_property_keyword() {
        let input = schema(
            json!({ "a": { "type": "string", "format": "uri" } }),
            json!([]),
            false,
        );
        let err = validate_input_schema(&input).unwrap_err();
        assert!(err.contains("unsupported keyword"));
    }
}

use axum::http::StatusCode;
use chrono::{DateTime, NaiveDate};
use serde_json::Value;
use uuid::Uuid;

/// Validates the model-plane property contract at an object or relationship write boundary.
/// Existing primitive schemas remain valid, while richer definitions add semantic validation
/// without making source projections reject fields the model has not declared yet.
pub fn validate_properties(schema: &Value, properties: &Value) -> Result<(), StatusCode> {
    let Some(schema) = schema.as_object() else {
        return if schema.is_null() { Ok(()) } else { Err(StatusCode::BAD_REQUEST) };
    };
    let Some(properties) = properties.as_object() else {
        return Err(StatusCode::BAD_REQUEST);
    };
    for (name, definition) in schema {
        let definition = definition.as_object().ok_or(StatusCode::BAD_REQUEST)?;
        validate_definition_metadata(definition)?;
        let required = definition.get("required").and_then(Value::as_bool).unwrap_or(false);
        let Some(value) = properties.get(name) else {
            if required {
                return Err(StatusCode::BAD_REQUEST);
            }
            continue;
        };
        validate_value(definition, value)?;
    }
    Ok(())
}

/// Ensures a model definition can be safely persisted before it is used to
/// validate projections. This keeps invalid rich contracts out of versioned
/// model history instead of discovering them only during a later write.
pub fn validate_schema(schema: &Value) -> Result<(), StatusCode> {
    let Some(schema) = schema.as_object() else {
        return if schema.is_null() { Ok(()) } else { Err(StatusCode::BAD_REQUEST) };
    };
    for definition in schema.values() {
        validate_definition(definition.as_object().ok_or(StatusCode::BAD_REQUEST)?)?;
    }
    Ok(())
}

fn validate_definition(definition: &serde_json::Map<String, Value>) -> Result<(), StatusCode> {
    validate_definition_metadata(definition)?;
    if definition.get("required").is_some_and(|value| !value.is_boolean()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let type_ = definition.get("type").and_then(Value::as_str).ok_or(StatusCode::BAD_REQUEST)?;
    if !matches!(
        type_,
        "string"
            | "number"
            | "float"
            | "integer"
            | "boolean"
            | "uuid"
            | "reference"
            | "decimal"
            | "date"
            | "timestamp"
            | "money"
            | "file"
            | "blob"
            | "image"
            | "vector"
            | "coordinate"
            | "enum"
            | "object"
            | "array"
    ) {
        return Err(StatusCode::BAD_REQUEST);
    }
    if definition.get("minimum").is_some_and(|value| !value.is_number())
        || definition.get("maximum").is_some_and(|value| !value.is_number())
        || definition.get("min_length").is_some_and(|value| !value.is_u64())
        || definition.get("max_length").is_some_and(|value| !value.is_u64())
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    if let (Some(minimum), Some(maximum)) = (
        definition.get("minimum").and_then(Value::as_f64),
        definition.get("maximum").and_then(Value::as_f64),
    ) {
        if minimum > maximum {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    match type_ {
        "enum" if definition.get("values").and_then(Value::as_array).is_none_or(Vec::is_empty) => {
            return Err(StatusCode::BAD_REQUEST);
        }
        "vector" => {
            if definition
                .get("dimensions")
                .and_then(Value::as_u64)
                .is_some_and(|dimensions| dimensions == 0)
            {
                return Err(StatusCode::BAD_REQUEST);
            }
            if definition.get("dimensions").is_some_and(|value| !value.is_u64()) {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        "array" => {
            if let Some(items) = definition.get("items") {
                validate_definition(items.as_object().ok_or(StatusCode::BAD_REQUEST)?)?;
            }
        }
        "object" => {
            if let Some(properties) = definition.get("properties") {
                validate_schema(properties)?;
            }
        }
        "reference"
            if definition.get("target_type").is_some_and(|value| {
                value.as_str().filter(|value| !value.is_empty()).is_none()
            }) =>
        {
            return Err(StatusCode::BAD_REQUEST);
        }
        _ => {}
    }
    Ok(())
}

fn validate_definition_metadata(
    definition: &serde_json::Map<String, Value>,
) -> Result<(), StatusCode> {
    let valid_string = |key| {
        definition
            .get(key)
            .map(|value| value.as_str().filter(|value| !value.is_empty()).is_some())
            .unwrap_or(true)
    };
    if !valid_string("sensitivity")
        || !valid_string("source_ownership")
        || !valid_string("write_policy")
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    for key in ["display", "provenance"] {
        if definition.get(key).is_some_and(|value| !value.is_object()) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    Ok(())
}

fn validate_value(
    definition: &serde_json::Map<String, Value>,
    value: &Value,
) -> Result<(), StatusCode> {
    let expected = definition.get("type").and_then(Value::as_str).ok_or(StatusCode::BAD_REQUEST)?;
    let valid = match expected {
        "string" => value.is_string(),
        "number" | "float" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "boolean" => value.is_boolean(),
        "uuid" | "reference" => {
            value.as_str().and_then(|value| Uuid::parse_str(value).ok()).is_some()
        }
        "decimal" => value.as_str().is_some_and(is_decimal),
        "date" => value
            .as_str()
            .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
            .is_some(),
        "timestamp" => {
            value.as_str().and_then(|value| DateTime::parse_from_rfc3339(value).ok()).is_some()
        }
        "money" => is_money(value),
        "file" | "blob" => is_file_reference(value, None),
        "image" => is_file_reference(value, Some("image/")),
        "vector" => is_vector(definition, value),
        "coordinate" => is_coordinate(value),
        "enum" => definition
            .get("values")
            .and_then(Value::as_array)
            .is_some_and(|values| values.contains(value)),
        "object" => validate_object(definition, value).is_ok(),
        "array" => validate_array(definition, value).is_ok(),
        _ => false,
    };
    if !valid {
        return Err(StatusCode::BAD_REQUEST);
    }
    validate_constraints(definition, value)
}

fn validate_object(
    definition: &serde_json::Map<String, Value>,
    value: &Value,
) -> Result<(), StatusCode> {
    let Some(properties) = definition.get("properties") else {
        return Ok(());
    };
    validate_properties(properties, value)
}

fn validate_array(
    definition: &serde_json::Map<String, Value>,
    value: &Value,
) -> Result<(), StatusCode> {
    let values = value.as_array().ok_or(StatusCode::BAD_REQUEST)?;
    if let Some(items) = definition.get("items") {
        let item_definition = items.as_object().ok_or(StatusCode::BAD_REQUEST)?;
        for item in values {
            validate_value(item_definition, item)?;
        }
    }
    Ok(())
}

fn validate_constraints(
    definition: &serde_json::Map<String, Value>,
    value: &Value,
) -> Result<(), StatusCode> {
    if let Some(minimum) = definition.get("minimum").and_then(Value::as_f64) {
        if value.as_f64().is_none_or(|number| number < minimum) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    if let Some(maximum) = definition.get("maximum").and_then(Value::as_f64) {
        if value.as_f64().is_none_or(|number| number > maximum) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    let length = value.as_str().map(str::len).or_else(|| value.as_array().map(Vec::len));
    if let Some(min_length) = definition.get("min_length").and_then(Value::as_u64) {
        if length.is_none_or(|length| length < min_length as usize) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    if let Some(max_length) = definition.get("max_length").and_then(Value::as_u64) {
        if length.is_none_or(|length| length > max_length as usize) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    Ok(())
}

fn is_decimal(value: &str) -> bool {
    let digits = value.strip_prefix('-').unwrap_or(value);
    let mut parts = digits.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    !whole.is_empty()
        && whole.bytes().all(|byte| byte.is_ascii_digit())
        && fraction.is_none_or(|fraction| {
            !fraction.is_empty() && fraction.bytes().all(|byte| byte.is_ascii_digit())
        })
        && parts.next().is_none()
}

fn is_money(value: &Value) -> bool {
    let Some(value) = value.as_object() else {
        return false;
    };
    value.get("amount").and_then(Value::as_str).is_some_and(is_decimal)
        && value.get("currency").and_then(Value::as_str).is_some_and(|currency| {
            currency.len() == 3 && currency.bytes().all(|byte| byte.is_ascii_uppercase())
        })
}

fn is_file_reference(value: &Value, content_type_prefix: Option<&str>) -> bool {
    let Some(value) = value.as_object() else {
        return false;
    };
    let required = ["uri", "sha256", "content_type"];
    if !required
        .iter()
        .all(|key| value.get(*key).and_then(Value::as_str).is_some_and(|item| !item.is_empty()))
    {
        return false;
    }
    content_type_prefix.is_none_or(|prefix| {
        value["content_type"].as_str().is_some_and(|type_| type_.starts_with(prefix))
    })
}

fn is_vector(definition: &serde_json::Map<String, Value>, value: &Value) -> bool {
    let Some(values) = value.as_array() else {
        return false;
    };
    definition
        .get("dimensions")
        .and_then(Value::as_u64)
        .is_none_or(|dimensions| values.len() == dimensions as usize)
        && values.iter().all(Value::is_number)
}

fn is_coordinate(value: &Value) -> bool {
    let Some(value) = value.as_object() else {
        return false;
    };
    let latitude = value.get("latitude").and_then(Value::as_f64);
    let longitude = value.get("longitude").and_then(Value::as_f64);
    latitude.is_some_and(|latitude| (-90.0..=90.0).contains(&latitude))
        && longitude.is_some_and(|longitude| (-180.0..=180.0).contains(&longitude))
}

#[cfg(test)]
#[path = "property_contract_test.rs"]
mod property_contract_test;

//! The check of an answer against its JSON Schema (guarantee 1). The runtime already follows
//! the schema while generating; this check is the second line, for the subset of JSON Schema
//! the engine's tasks use: `type` (one or a list), `properties`, `required`,
//! `additionalProperties: false`, `items`, `enum`, `minimum`, `maximum`.

use serde_json::Value;

/// Parse `text` and check it against `schema`. Returns the value or what is wrong.
///
/// # Errors
/// Not JSON, or not matching the schema.
pub fn parse_checked(text: &str, schema: &Value) -> Result<Value, String> {
    let v: Value = serde_json::from_str(text.trim()).map_err(|e| format!("not JSON: {e}"))?;
    check(&v, schema, "$")?;
    Ok(v)
}

fn type_ok(v: &Value, t: &str) -> bool {
    match t {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "number" => v.is_number(),
        "integer" => v.as_f64().is_some_and(|f| f.fract() == 0.0),
        "boolean" => v.is_boolean(),
        "null" => v.is_null(),
        _ => true,
    }
}

/// Check one value.
///
/// # Errors
/// Where and why it does not match.
pub fn check(v: &Value, s: &Value, at: &str) -> Result<(), String> {
    match s.get("type") {
        Some(Value::String(t)) if !type_ok(v, t) => return Err(format!("{at}: expected {t}")),
        Some(Value::Array(ts)) if !ts.iter().filter_map(Value::as_str).any(|t| type_ok(v, t)) => {
            return Err(format!("{at}: wrong type"));
        }
        _ => {}
    }
    if let Some(Value::Array(options)) = s.get("enum")
        && !options.contains(v)
    {
        return Err(format!("{at}: not one of the allowed values"));
    }
    if let Some(n) = v.as_f64() {
        if s.get("minimum")
            .and_then(Value::as_f64)
            .is_some_and(|m| n < m)
        {
            return Err(format!("{at}: below the minimum"));
        }
        if s.get("maximum")
            .and_then(Value::as_f64)
            .is_some_and(|m| n > m)
        {
            return Err(format!("{at}: above the maximum"));
        }
    }
    if let Some(o) = v.as_object() {
        let props = s.get("properties").and_then(Value::as_object);
        if let Some(req) = s.get("required").and_then(Value::as_array) {
            for k in req.iter().filter_map(Value::as_str) {
                if !o.contains_key(k) {
                    return Err(format!("{at}: missing {k}"));
                }
            }
        }
        for (k, x) in o {
            match props.and_then(|p| p.get(k)) {
                Some(ps) => check(x, ps, &format!("{at}.{k}"))?,
                None if s.get("additionalProperties") == Some(&Value::Bool(false)) => {
                    return Err(format!("{at}: unexpected {k}"));
                }
                None => {}
            }
        }
    }
    if let (Some(a), Some(items)) = (v.as_array(), s.get("items")) {
        for (i, x) in a.iter().enumerate() {
            check(x, items, &format!("{at}[{i}]"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn checks_the_used_subset() {
        let s = json!({
            "type": "object",
            "properties": {
                "kind": { "type": "string", "enum": ["a", "b"] },
                "value": { "type": ["string", "null"] },
                "score": { "type": "number", "minimum": 0, "maximum": 1 },
                "tags": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["kind", "value"],
            "additionalProperties": false
        });
        assert!(parse_checked(r#"{"kind":"a","value":null,"score":0.5,"tags":["x"]}"#, &s).is_ok());
        assert!(parse_checked(r#"{"kind":"c","value":null}"#, &s).is_err());
        assert!(parse_checked(r#"{"kind":"a"}"#, &s).is_err());
        assert!(parse_checked(r#"{"kind":"a","value":"v","more":1}"#, &s).is_err());
        assert!(parse_checked(r#"{"kind":"a","value":"v","score":2}"#, &s).is_err());
        assert!(parse_checked(r#"{"kind":"a","value":"v","tags":[1]}"#, &s).is_err());
        assert!(parse_checked("not json", &s).is_err());
    }
}

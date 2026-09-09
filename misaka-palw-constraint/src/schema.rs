//! **The JSON-Schema subset a free-prompt answer may be asked to take** (ADR-0096 Decision 7),
//! parsed with every keyword outside it refused BY NAME, and a validator over it.
//!
//! The table, from the ADR: `type` in {object, array, string, number, integer, boolean, null}
//! (one or a list), `properties`, `required`, `additionalProperties` (`true`, `false`, or a
//! schema), `items` (a schema), `minItems`/`maxItems`, `enum`, `const`, `pattern`,
//! `minLength`/`maxLength`, `minimum`/`maximum`, nesting to [`MAX_SCHEMA_DEPTH`].
//! `description`, `title` and `$schema` are ignored (they say nothing about an answer). Anything
//! else — `$ref`, `$defs`, `oneOf`, `anyOf`, `allOf`, `not`, `if`/`then`/`else`, `format`,
//! `patternProperties`, `dependentRequired`, `prefixItems`, `default`, `examples`, … — is a
//! refusal that names the keyword and the path it sits at.
//!
//! **Why refuse rather than ignore a keyword.** A schema with `oneOf` beside `type: object` that
//! validated against `type` alone would report `valid: true` for an answer the person's own
//! schema rejects; and under Part B the same schema would have to compile, which it cannot. The
//! subset is the intersection of what the entrance can validate and what the compiler will be
//! able to admit token by token, and a keyword outside it is outside it in both modes.
//!
//! **`pattern`.** Validated with the `regex` crate the workspace already pins. JSON Schema's
//! patterns are ECMA-262 dialect; `regex` is not (no lookaround, no backreferences), and a pattern
//! it cannot compile is refused by name rather than approximated. Decision 7's "regex subset the
//! crate pins by table" is Part B's automaton compiler's — until it lands, the table is "what
//! `regex` accepts", and an integration should keep to the common core (character classes,
//! quantifiers, anchors, alternation).
//!
//! **Equality for `enum` and `const`** is JSON equality, decided over RFC 8785 bytes so that `1`
//! and `1.0` are one value (as JSON Schema says) and member order never matters.

use std::fmt::Write as _;

use serde_json::Value;

use crate::canonical::to_rfc8785;

/// The deepest a schema may nest (`properties` inside `items` inside `properties` …). Pinned so a
/// schema is a bounded object, which is what a decode constraint has to be.
pub const MAX_SCHEMA_DEPTH: usize = 16;

/// The keywords the subset carries.
const KNOWN_KEYWORDS: &[&str] = &[
    "type",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "minItems",
    "maxItems",
    "enum",
    "const",
    "pattern",
    "minLength",
    "maxLength",
    "minimum",
    "maximum",
];
/// Keywords that say nothing about an answer and are ignored, by name, rather than refused.
const IGNORED_KEYWORDS: &[&str] = &["description", "title", "$schema"];

/// A JSON type name the subset admits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonType {
    Object,
    Array,
    String,
    Number,
    Integer,
    Boolean,
    Null,
}

impl JsonType {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "object" => Self::Object,
            "array" => Self::Array,
            "string" => Self::String,
            "number" => Self::Number,
            "integer" => Self::Integer,
            "boolean" => Self::Boolean,
            "null" => Self::Null,
            _ => return None,
        })
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Object => "object",
            Self::Array => "array",
            Self::String => "string",
            Self::Number => "number",
            Self::Integer => "integer",
            Self::Boolean => "boolean",
            Self::Null => "null",
        }
    }

    /// Does `value` have this type? `integer` is a number whose value is integral — `1.0` is an
    /// integer to JSON Schema, and `1e2` is too.
    pub fn matches(self, value: &Value) -> bool {
        match (self, value) {
            (Self::Object, Value::Object(_))
            | (Self::Array, Value::Array(_))
            | (Self::String, Value::String(_))
            | (Self::Number, Value::Number(_))
            | (Self::Boolean, Value::Bool(_))
            | (Self::Null, Value::Null) => true,
            (Self::Integer, Value::Number(n)) => {
                n.is_i64() || n.is_u64() || n.as_f64().is_some_and(|f| f.is_finite() && f.fract() == 0.0)
            }
            _ => false,
        }
    }
}

/// What the type of a value is called in an error.
fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// `additionalProperties`, the three forms the subset admits.
#[derive(Clone, Debug)]
pub enum AdditionalProperties {
    /// Absent or `true`: any other member is fine.
    Allowed,
    /// `false`: a member `properties` does not name is an error.
    Forbidden,
    /// A schema every other member must satisfy.
    Schema(Box<Schema>),
}

/// A compiled `pattern`: the source the person wrote, and the regex it compiled to.
#[derive(Clone, Debug)]
pub struct Pattern {
    pub source: String,
    pub regex: regex::Regex,
}

/// A parsed schema in the subset. Every field is what the keyword says; absent keywords are
/// `None`/empty and constrain nothing.
#[derive(Clone, Debug, Default)]
pub struct Schema {
    /// `None` is "any type".
    pub types: Option<Vec<JsonType>>,
    /// Sorted by name — the map's order is the client's business and the errors' order is not.
    pub properties: Vec<(String, Schema)>,
    pub required: Vec<String>,
    pub additional_properties: Option<AdditionalProperties>,
    pub items: Option<Box<Schema>>,
    pub min_items: Option<u64>,
    pub max_items: Option<u64>,
    pub enumeration: Option<Vec<Value>>,
    pub constant: Option<Value>,
    pub pattern: Option<Pattern>,
    pub min_length: Option<u64>,
    pub max_length: Option<u64>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
}

/// Parse a schema, refusing anything outside the subset by name and path.
pub fn parse(value: &Value) -> Result<Schema, String> {
    parse_at(value, "", 0)
}

/// RFC 6901: `~` is `~0` and `/` is `~1` inside a reference token.
fn pointer_token(name: &str) -> String {
    name.replace('~', "~0").replace('/', "~1")
}

fn parse_at(value: &Value, path: &str, depth: usize) -> Result<Schema, String> {
    if depth > MAX_SCHEMA_DEPTH {
        return Err(format!(
            "the schema nests deeper than {MAX_SCHEMA_DEPTH} at {path:?} (ADR-0096 Decision 7 pins the depth; a decode constraint is a bounded object)"
        ));
    }
    let members = value.as_object().ok_or_else(|| {
        format!(
            "the schema at {path:?} is {} where an object was expected (a boolean schema is outside the subset; spell `true` as {{}} and `false` as an `enum` of nothing)",
            kind_of(value)
        )
    })?;
    // Every keyword is classified BEFORE any is read: a schema that carries `oneOf` beside `type`
    // must not validate against `type` alone.
    for key in members.keys() {
        if !KNOWN_KEYWORDS.contains(&key.as_str()) && !IGNORED_KEYWORDS.contains(&key.as_str()) {
            return Err(format!(
                "`{key}` at {path:?} is outside the JSON-Schema subset this lane compiles (ADR-0096 Decision 7): $ref, $defs, oneOf, \
                 anyOf, allOf, not, if/then/else, format, patternProperties, dependentRequired, prefixItems and every other keyword \
                 are refused rather than approximated; the subset is type, properties, required, additionalProperties, items, \
                 minItems, maxItems, enum, const, pattern, minLength, maxLength, minimum, maximum"
            ));
        }
    }

    let mut schema = Schema::default();
    if let Some(t) = members.get("type") {
        let names: Vec<&str> = match t {
            Value::String(s) => vec![s.as_str()],
            Value::Array(items) => items
                .iter()
                .map(|item| {
                    item.as_str().ok_or_else(|| format!("`type` at {path:?} lists a {} where a type name was expected", kind_of(item)))
                })
                .collect::<Result<_, _>>()?,
            other => return Err(format!("`type` at {path:?} is {} where a type name or a list of them was expected", kind_of(other))),
        };
        if names.is_empty() {
            return Err(format!("`type` at {path:?} is an empty list, which admits nothing"));
        }
        let mut types = Vec::with_capacity(names.len());
        for name in names {
            let parsed = JsonType::parse(name).ok_or_else(|| {
                format!("`type` {name:?} at {path:?} is not one of object, array, string, number, integer, boolean, null")
            })?;
            if types.contains(&parsed) {
                return Err(format!("`type` at {path:?} names {name:?} twice"));
            }
            types.push(parsed);
        }
        schema.types = Some(types);
    }
    if let Some(p) = members.get("properties") {
        let props = p.as_object().ok_or_else(|| format!("`properties` at {path:?} is {} where an object was expected", kind_of(p)))?;
        let mut parsed: Vec<(String, Schema)> = Vec::with_capacity(props.len());
        for (name, sub) in props {
            let sub_path = format!("{path}/properties/{}", pointer_token(name));
            parsed.push((name.clone(), parse_at(sub, &sub_path, depth + 1)?));
        }
        parsed.sort_by(|a, b| a.0.cmp(&b.0));
        schema.properties = parsed;
    }
    if let Some(r) = members.get("required") {
        let items =
            r.as_array().ok_or_else(|| format!("`required` at {path:?} is {} where a list of names was expected", kind_of(r)))?;
        let mut required = Vec::with_capacity(items.len());
        for item in items {
            let name = item
                .as_str()
                .ok_or_else(|| format!("`required` at {path:?} lists a {} where a property name was expected", kind_of(item)))?;
            if !required.iter().any(|r: &String| r == name) {
                required.push(name.to_string());
            }
        }
        schema.required = required;
    }
    if let Some(a) = members.get("additionalProperties") {
        schema.additional_properties = Some(match a {
            Value::Bool(true) => AdditionalProperties::Allowed,
            Value::Bool(false) => AdditionalProperties::Forbidden,
            Value::Object(_) => {
                AdditionalProperties::Schema(Box::new(parse_at(a, &format!("{path}/additionalProperties"), depth + 1)?))
            }
            other => {
                return Err(format!(
                    "`additionalProperties` at {path:?} is {} where true, false or a schema was expected",
                    kind_of(other)
                ));
            }
        });
    }
    if let Some(i) = members.get("items") {
        if !i.is_object() {
            return Err(format!(
                "`items` at {path:?} is {} where a schema was expected (a boolean `items` and `prefixItems` are outside the subset)",
                kind_of(i)
            ));
        }
        schema.items = Some(Box::new(parse_at(i, &format!("{path}/items"), depth + 1)?));
    }
    schema.min_items = count_keyword(members, "minItems", path)?;
    schema.max_items = count_keyword(members, "maxItems", path)?;
    schema.min_length = count_keyword(members, "minLength", path)?;
    schema.max_length = count_keyword(members, "maxLength", path)?;
    if let (Some(lo), Some(hi)) = (schema.min_items, schema.max_items)
        && lo > hi
    {
        return Err(format!("`minItems` {lo} exceeds `maxItems` {hi} at {path:?}, which admits nothing"));
    }
    if let (Some(lo), Some(hi)) = (schema.min_length, schema.max_length)
        && lo > hi
    {
        return Err(format!("`minLength` {lo} exceeds `maxLength` {hi} at {path:?}, which admits nothing"));
    }
    if let Some(e) = members.get("enum") {
        let items = e.as_array().ok_or_else(|| format!("`enum` at {path:?} is {} where a list of values was expected", kind_of(e)))?;
        if items.is_empty() {
            return Err(format!("`enum` at {path:?} is empty, which admits nothing"));
        }
        for item in items {
            to_rfc8785(item).map_err(|why| format!("`enum` at {path:?} holds a value with no canonical form: {why}"))?;
        }
        schema.enumeration = Some(items.clone());
    }
    if let Some(c) = members.get("const") {
        to_rfc8785(c).map_err(|why| format!("`const` at {path:?} has no canonical form: {why}"))?;
        schema.constant = Some(c.clone());
    }
    if let Some(p) = members.get("pattern") {
        let source = p.as_str().ok_or_else(|| format!("`pattern` at {path:?} is {} where a regex was expected", kind_of(p)))?;
        let regex = regex::Regex::new(source).map_err(|e| {
            format!("`pattern` at {path:?} is not a regex this lane's dialect accepts (the `regex` crate: no lookaround, no backreferences): {e}")
        })?;
        schema.pattern = Some(Pattern { source: source.to_string(), regex });
    }
    schema.minimum = number_keyword(members, "minimum", path)?;
    schema.maximum = number_keyword(members, "maximum", path)?;
    if let (Some(lo), Some(hi)) = (schema.minimum, schema.maximum)
        && lo > hi
    {
        return Err(format!("`minimum` {lo} exceeds `maximum` {hi} at {path:?}, which admits nothing"));
    }
    Ok(schema)
}

fn count_keyword(members: &serde_json::Map<String, Value>, keyword: &str, path: &str) -> Result<Option<u64>, String> {
    match members.get(keyword) {
        None => Ok(None),
        Some(v) => {
            v.as_u64().map(Some).ok_or_else(|| format!("`{keyword}` at {path:?} is {v} where a non-negative integer was expected"))
        }
    }
}

fn number_keyword(members: &serde_json::Map<String, Value>, keyword: &str, path: &str) -> Result<Option<f64>, String> {
    match members.get(keyword) {
        None => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|f| f.is_finite())
            .map(Some)
            .ok_or_else(|| format!("`{keyword}` at {path:?} is {v} where a number was expected")),
    }
}

/// Validate `value` against `schema`. `Err` carries EVERY violation found, each at its
/// JSON-pointer path (RFC 6901; the root is `""`), so a person fixing an answer sees the whole
/// list rather than the first item of it.
pub fn validate(schema: &Schema, value: &Value) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    validate_at(schema, value, "", &mut errors);
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

fn json_equal(a: &Value, b: &Value) -> bool {
    match (to_rfc8785(a), to_rfc8785(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn validate_at(schema: &Schema, value: &Value, path: &str, errors: &mut Vec<String>) {
    if let Some(types) = &schema.types
        && !types.iter().any(|t| t.matches(value))
    {
        let wanted: Vec<&str> = types.iter().map(|t| t.name()).collect();
        errors.push(format!("at {path:?}: expected {}, got {}", wanted.join(" or "), kind_of(value)));
    }
    if let Some(values) = &schema.enumeration
        && !values.iter().any(|candidate| json_equal(candidate, value))
    {
        errors.push(format!("at {path:?}: the value is not one of the enum's {} values", values.len()));
    }
    if let Some(constant) = &schema.constant
        && !json_equal(constant, value)
    {
        let mut spelled = String::new();
        write!(spelled, "{}", String::from_utf8_lossy(&to_rfc8785(constant).unwrap_or_default())).expect("String");
        errors.push(format!("at {path:?}: the value is not the const {spelled}"));
    }
    match value {
        Value::Object(members) => {
            for name in &schema.required {
                if !members.contains_key(name) {
                    errors.push(format!("at {path:?}: required property `{name}` is missing"));
                }
            }
            for (name, member) in members {
                let member_path = format!("{path}/{}", pointer_token(name));
                match schema.properties.iter().find(|(declared, _)| declared == name) {
                    Some((_, sub)) => validate_at(sub, member, &member_path, errors),
                    None => match &schema.additional_properties {
                        None | Some(AdditionalProperties::Allowed) => {}
                        Some(AdditionalProperties::Forbidden) => {
                            errors.push(format!("at {member_path:?}: additional property `{name}` is not allowed"));
                        }
                        Some(AdditionalProperties::Schema(sub)) => validate_at(sub, member, &member_path, errors),
                    },
                }
            }
        }
        Value::Array(items) => {
            let n = items.len() as u64;
            if let Some(min) = schema.min_items
                && n < min
            {
                errors.push(format!("at {path:?}: {n} items where at least {min} were required"));
            }
            if let Some(max) = schema.max_items
                && n > max
            {
                errors.push(format!("at {path:?}: {n} items where at most {max} were allowed"));
            }
            if let Some(sub) = &schema.items {
                for (index, item) in items.iter().enumerate() {
                    validate_at(sub, item, &format!("{path}/{index}"), errors);
                }
            }
        }
        Value::String(s) => {
            // JSON Schema counts characters (code points), not bytes.
            let n = s.chars().count() as u64;
            if let Some(min) = schema.min_length
                && n < min
            {
                errors.push(format!("at {path:?}: {n} characters where at least {min} were required"));
            }
            if let Some(max) = schema.max_length
                && n > max
            {
                errors.push(format!("at {path:?}: {n} characters where at most {max} were allowed"));
            }
            if let Some(pattern) = &schema.pattern
                && !pattern.regex.is_match(s)
            {
                errors.push(format!("at {path:?}: the string does not match the pattern {:?}", pattern.source));
            }
        }
        Value::Number(n) => {
            // serde_json holds neither NaN nor an infinity, so every number has a finite double
            // value and the comparisons below are ordinary.
            let f = n.as_f64().unwrap_or_default();
            if let Some(min) = schema.minimum
                && f < min
            {
                errors.push(format!("at {path:?}: {n} is below the minimum {}", crate::canonical::es_number_to_string(min)));
            }
            if let Some(max) = schema.maximum
                && f > max
            {
                errors.push(format!("at {path:?}: {n} is above the maximum {}", crate::canonical::es_number_to_string(max)));
            }
        }
        Value::Bool(_) | Value::Null => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema(v: Value) -> Schema {
        parse(&v).unwrap_or_else(|e| panic!("{v} should parse: {e}"))
    }

    /// The whole table parses, and validates a conforming answer with no errors.
    #[test]
    fn the_subset_parses_and_a_conforming_answer_validates() {
        let s = schema(json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "Person",
            "description": "ignored",
            "type": "object",
            "properties": {
                "name": { "type": "string", "minLength": 1, "maxLength": 64, "pattern": "^[A-Z]" },
                "age": { "type": "integer", "minimum": 0, "maximum": 150 },
                "tags": { "type": "array", "items": { "type": "string" }, "minItems": 0, "maxItems": 4 },
                "kind": { "enum": ["person", "robot"] },
                "version": { "const": 2 },
                "nickname": { "type": ["string", "null"] },
                "extra": { "type": "object", "additionalProperties": { "type": "number" } }
            },
            "required": ["name", "age", "kind", "version"],
            "additionalProperties": false
        }));
        assert_eq!(s.types, Some(vec![JsonType::Object]));
        assert_eq!(s.properties.len(), 7);
        assert!(s.properties.windows(2).all(|w| w[0].0 < w[1].0), "properties are sorted by name");
        assert_eq!(s.required, vec!["name", "age", "kind", "version"]);
        assert!(matches!(s.additional_properties, Some(AdditionalProperties::Forbidden)));
        validate(
            &s,
            &json!({ "name": "Ada", "age": 36.0, "tags": ["x"], "kind": "person", "version": 2.0, "nickname": null, "extra": { "h": 1.5 } }),
        )
        .expect("a conforming answer");
    }

    /// Every keyword outside the subset is refused by NAME and PATH — including the ones a schema
    /// generator emits by default (`$defs`+`$ref`, `default`, `format`) — and a schema that carries
    /// one beside a keyword we do understand is refused too, never half-validated.
    #[test]
    fn keywords_outside_the_subset_are_refused_by_name_and_path() {
        let cases: Vec<(Value, &str)> = vec![
            (json!({"$ref": "#/$defs/x"}), "`$ref` at \"\""),
            (json!({"type": "object", "properties": {"a": {"oneOf": [{"type": "string"}]}}}), "`oneOf` at \"/properties/a\""),
            (json!({"anyOf": []}), "`anyOf`"),
            (json!({"allOf": []}), "`allOf`"),
            (json!({"not": {}}), "`not`"),
            (json!({"if": {}}), "`if`"),
            (json!({"type": "string", "format": "email"}), "`format`"),
            (json!({"patternProperties": {}}), "`patternProperties`"),
            (json!({"dependentRequired": {}}), "`dependentRequired`"),
            (json!({"type": "array", "prefixItems": []}), "`prefixItems`"),
            (json!({"type": "string", "default": "x"}), "`default`"),
            (json!({"$defs": {}}), "`$defs`"),
            (
                json!({"type": "object", "items": {"type": "object", "properties": {"deep": {"$ref": "#"}}}}),
                "at \"/items/properties/deep\"",
            ),
        ];
        for (value, needle) in cases {
            let err = parse(&value).expect_err(&format!("{value} must be refused"));
            assert!(err.contains(needle), "the refusal must name {needle:?}: {err}");
            assert!(err.contains("ADR-0096 Decision 7"), "and the rule: {err}");
        }
    }

    /// Malformed uses of keywords we DO understand are refused with the keyword's name.
    #[test]
    fn malformed_subset_keywords_are_refused() {
        let cases: Vec<(Value, &str)> = vec![
            (json!(true), "boolean schema"),
            (json!({"type": "strng"}), "`type` \"strng\""),
            (json!({"type": []}), "empty list"),
            (json!({"type": ["string", "string"]}), "twice"),
            (json!({"type": 3}), "`type` at \"\" is number"),
            (json!({"properties": []}), "`properties`"),
            (json!({"required": "name"}), "`required`"),
            (json!({"required": [1]}), "`required`"),
            (json!({"additionalProperties": "no"}), "`additionalProperties`"),
            (json!({"items": true}), "`items`"),
            (json!({"minItems": -1}), "`minItems`"),
            (json!({"maxLength": 1.5}), "`maxLength`"),
            (json!({"minItems": 3, "maxItems": 2}), "`minItems` 3 exceeds `maxItems` 2"),
            (json!({"minimum": 2, "maximum": 1}), "`minimum` 2 exceeds `maximum` 1"),
            (json!({"enum": []}), "`enum`"),
            (json!({"enum": "a"}), "`enum`"),
            (json!({"pattern": 4}), "`pattern`"),
            (json!({"pattern": "(?<=a)b"}), "`pattern`"),
            (json!({"pattern": "(unclosed"}), "not a regex"),
            (json!({"minimum": "0"}), "`minimum`"),
        ];
        for (value, needle) in cases {
            let err = parse(&value).expect_err(&format!("{value} must be refused"));
            assert!(err.contains(needle), "the refusal must name {needle:?}: {err}");
        }
    }

    /// Depth is bounded: sixteen levels parse, the seventeenth is refused with the depth.
    #[test]
    fn nesting_is_bounded_at_the_pinned_depth() {
        fn nested(levels: usize) -> Value {
            let mut v = json!({"type": "string"});
            for _ in 0..levels {
                v = json!({"type": "object", "properties": {"n": v}});
            }
            v
        }
        assert!(parse(&nested(MAX_SCHEMA_DEPTH)).is_ok(), "{MAX_SCHEMA_DEPTH} levels are admissible");
        let err = parse(&nested(MAX_SCHEMA_DEPTH + 1)).unwrap_err();
        assert!(err.contains(&format!("deeper than {MAX_SCHEMA_DEPTH}")), "{err}");
        assert_eq!(MAX_SCHEMA_DEPTH, 16);
    }

    /// Every violation is reported, at its JSON-pointer path, and the list is the whole list.
    #[test]
    fn violations_are_reported_at_json_pointer_paths() {
        let s = schema(json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "minLength": 2, "pattern": "^[a-z]+$" },
                "n": { "type": "integer", "minimum": 0, "maximum": 10 },
                "list": { "type": "array", "items": { "type": "number" }, "maxItems": 2 },
                "a/b": { "const": "x" },
                "k": { "enum": [1, "two"] }
            },
            "required": ["name", "n"],
            "additionalProperties": false
        }));
        let errors =
            validate(&s, &json!({ "name": "A", "n": 10.5, "list": [1, "s", 3], "a/b": "y", "k": 2, "zzz": true })).unwrap_err();
        let expected = [
            "at \"/name\": 1 characters where at least 2 were required",
            "at \"/name\": the string does not match the pattern \"^[a-z]+$\"",
            "at \"/n\": expected integer, got number",
            "at \"/n\": 10.5 is above the maximum 10",
            "at \"/list\": 3 items where at most 2 were allowed",
            "at \"/list/1\": expected number, got string",
            "at \"/a~1b\": the value is not the const \"x\"",
            "at \"/k\": the value is not one of the enum's 2 values",
            "at \"/zzz\": additional property `zzz` is not allowed",
        ];
        for needle in expected {
            assert!(errors.iter().any(|e| e == needle), "missing {needle:?} in {errors:#?}");
        }
        assert_eq!(errors.len(), expected.len(), "{errors:#?}");

        let missing = validate(&s, &json!({})).unwrap_err();
        assert_eq!(missing, vec!["at \"\": required property `name` is missing", "at \"\": required property `n` is missing"]);
        let wrong_root = validate(&s, &json!("not an object")).unwrap_err();
        assert_eq!(wrong_root, vec!["at \"\": expected object, got string"]);
    }

    /// JSON equality, not serde equality: `1` and `1.0` are one value to `enum`, `const` and
    /// `integer`; strings count characters, not bytes; `additionalProperties` as a schema applies
    /// to the undeclared members only.
    #[test]
    fn equality_is_json_equality_and_lengths_are_characters() {
        let s = schema(json!({ "enum": [1, {"b": 2, "a": [1.0]}] }));
        validate(&s, &json!(1.0)).expect("1.0 is 1");
        validate(&s, &json!({"a": [1], "b": 2.0})).expect("member order and number spelling do not matter");
        assert!(validate(&s, &json!(2)).is_err());

        let c = schema(json!({ "const": 10 }));
        validate(&c, &json!(1e1)).expect("1e1 is 10");

        let i = schema(json!({ "type": "integer" }));
        validate(&i, &json!(3.0)).expect("3.0 is an integer");
        assert!(validate(&i, &json!(3.5)).is_err());

        let l = schema(json!({ "type": "string", "maxLength": 2 }));
        validate(&l, &json!("日本")).expect("two characters, six bytes");
        assert!(validate(&l, &json!("abc")).is_err());

        let ap = schema(
            json!({ "type": "object", "properties": { "id": { "type": "string" } }, "additionalProperties": { "type": "boolean" } }),
        );
        validate(&ap, &json!({ "id": "x", "flag": true })).expect("the undeclared member satisfies the schema");
        let err = validate(&ap, &json!({ "id": "x", "flag": "no" })).unwrap_err();
        assert_eq!(err, vec!["at \"/flag\": expected boolean, got string"]);
        // Absent `additionalProperties` allows anything, as the draft says.
        validate(&schema(json!({ "type": "object" })), &json!({ "anything": [1, 2] })).expect("open by default");
        // A `type` list admits any of its members.
        let nullable = schema(json!({ "type": ["string", "null"] }));
        validate(&nullable, &json!(null)).unwrap();
        validate(&nullable, &json!("s")).unwrap();
        assert_eq!(validate(&nullable, &json!(1)).unwrap_err(), vec!["at \"\": expected string or null, got number"]);
    }
}

//! JSON parsing that refuses duplicate keys at every nesting level.
use serde::{
    Deserialize, Deserializer,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Map, Number, Value};
use std::fmt;

struct Strict(Value);
impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StrictVisitor;
        impl<'de> Visitor<'de> for StrictVisitor {
            type Value = Strict;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("unambiguous JSON")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Strict, E> {
                Ok(Strict(Value::Bool(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Strict, E> {
                Number::from_f64(v)
                    .map(|n| Strict(Value::Number(n)))
                    .ok_or_else(|| E::custom("nonfinite number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Strict, E> {
                self.visit_string(v.to_owned())
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Strict, E> {
                if v.contains('\0') {
                    return Err(E::custom("NUL is not supported"));
                }
                Ok(Strict(Value::String(v)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Strict, E> {
                Ok(Strict(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Strict, A::Error> {
                let mut items = Vec::new();
                while let Some(Strict(value)) = seq.next_element()? {
                    items.push(value);
                }
                Ok(Strict(Value::Array(items)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Strict, A::Error> {
                let mut object = Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if key.contains('\0') || object.contains_key(&key) {
                        return Err(de::Error::custom("invalid or duplicate object key"));
                    }
                    object.insert(key, map.next_value::<Strict>()?.0);
                }
                Ok(Strict(Value::Object(object)))
            }
        }
        deserializer.deserialize_any(StrictVisitor)
    }
}

pub fn parse(bytes: &[u8], limit: usize) -> Result<Value, &'static str> {
    parse_detailed(bytes, limit).map_err(|code| match code {
        "E_PAYLOAD_LIMIT" => "E_PAYLOAD_LIMIT",
        _ => "E_INVALID_JSON",
    })
}

/// Categorize only fixed parser failures. The parser's message itself may
/// contain user content and must never be returned or logged.
pub fn parse_detailed(bytes: &[u8], limit: usize) -> Result<Value, &'static str> {
    if bytes.len() > limit {
        return Err("E_PAYLOAD_LIMIT");
    }
    serde_json::from_slice::<Strict>(bytes)
        .map(|v| v.0)
        .map_err(|error| {
            let message = error.to_string();
            if message.starts_with("invalid escape at line ") {
                "E_INVALID_JSON_ESCAPE"
            } else if message.starts_with("control character (") {
                "E_INVALID_JSON_CONTROL"
            } else {
                "E_INVALID_JSON"
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_keys_in_arguments_are_not_last_value_wins() {
        assert!(parse(br#"{"input":{"a":1,"a":2}}"#, 100).is_err());
        assert!(parse(br#"{"a":1} trailing"#, 100).is_err());
        assert!(parse(br#"{"a":"\u0000"}"#, 100).is_err());
        assert!(parse(br#"{"a":1}"#, 3).is_err());
    }

    #[test]
    fn diagnostics_do_not_export_invalid_string_content() {
        let bytes = br#"{"path":"C:\PRIVATE\file"}"#;
        assert_eq!(parse_detailed(bytes, 100), Err("E_INVALID_JSON_ESCAPE"));
        assert_eq!(parse(bytes, 100), Err("E_INVALID_JSON"));
        assert_eq!(
            parse_detailed(b"{\"text\":\"PRIVATE\nVALUE\"}", 100),
            Err("E_INVALID_JSON_CONTROL")
        );
        assert_eq!(
            parse_detailed(br#"{"path":"C:\u005cPRIVATE\u005cfile"}"#, 100).unwrap()["path"],
            r"C:\PRIVATE\file"
        );
    }
}

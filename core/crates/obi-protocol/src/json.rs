//! JSON as the protocol reads it (protocol.md section 2): UTF-8 without a byte order mark, and no
//! object with two members of the same name. `serde_json`'s own `Value` keeps the last of two such
//! members; a receiver MUST refuse the document instead, so everything obid reads from an Obysphere
//! or another member goes through [`from_slice`].

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::fmt;

/// `bytes` as JSON, refused when they aren't UTF-8, start with a byte order mark, or hold an
/// object with a member name twice (at any depth).
pub fn from_slice(bytes: &[u8]) -> Result<Value, String> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Err("a byte order mark isn't allowed".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "not UTF-8".to_owned())?;
    from_str(text)
}

/// [`from_slice`] for text already known to be UTF-8.
pub fn from_str(text: &str) -> Result<Value, String> {
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let Strict(value) = Strict::deserialize(&mut deserializer).map_err(|e| e.to_string())?;
    deserializer.end().map_err(|e| e.to_string())?;
    Ok(value)
}

/// A JSON value read with every object's member names checked once.
struct Strict(Value);

impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Strict, D::Error> {
        deserializer.deserialize_any(StrictVisitor).map(Strict)
    }
}

struct StrictVisitor;

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("a number JSON can't hold"))
    }

    fn visit_str<E>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }

    fn visit_string<E>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut out = Vec::new();
        while let Some(Strict(v)) = seq.next_element()? {
            out.push(v);
        }
        Ok(Value::Array(out))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Value, A::Error> {
        let mut out = Map::new();
        while let Some(key) = access.next_key::<String>()? {
            let Strict(v) = access.next_value()?;
            if out.insert(key.clone(), v).is_some() {
                return Err(de::Error::custom(format!(
                    "the member name {key:?} appears twice"
                )));
            }
        }
        Ok(Value::Object(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// What `serde_json` reads, this reads the same; a repeated member name, at any depth, is
    /// refused, where `serde_json` keeps the last.
    #[test]
    fn a_repeated_member_name_is_refused() {
        let plain = r#"{"alg":"EdDSA","kid":"k1","typ":"JOSE","n":[1,-2,3.5,null,true,{"a":"b"}]}"#;
        assert_eq!(
            from_str(plain).unwrap(),
            serde_json::from_str::<Value>(plain).unwrap()
        );
        let twice = r#"{"alg":"EdDSA","kid":"first","kid":"second","typ":"JOSE"}"#;
        assert_eq!(
            serde_json::from_str::<Value>(twice).unwrap()["kid"],
            "second"
        );
        assert!(
            from_str(twice)
                .unwrap_err()
                .contains("\"kid\" appears twice")
        );
        assert!(
            from_str(r#"{"a":[{"b":1,"b":1}]}"#).is_err(),
            "nested, the same value"
        );
        assert_eq!(
            from_str(r#"{"a":{"b":1},"b":{"a":1}}"#).unwrap(),
            json!({"a": {"b": 1}, "b": {"a": 1}})
        );
    }

    /// Bytes that aren't UTF-8 are refused, not replaced; so are a byte order mark, a lone
    /// surrogate and anything after the value.
    #[test]
    fn bytes_that_arent_utf8_are_refused() {
        let header = br#"{"alg":"EdDSA","kid":"k1","typ":"JOSE"}"#;
        assert!(from_slice(header).is_ok());
        let mut bad = header.to_vec();
        bad[17] = 0xFF;
        assert_eq!(from_slice(&bad).unwrap_err(), "not UTF-8");
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend_from_slice(header);
        assert!(from_slice(&bom).is_err());
        assert!(from_str(r#"{"a":"\ud800"}"#).is_err());
        assert!(from_str(r#"{"a":1} {"a":2}"#).is_err());
    }
}

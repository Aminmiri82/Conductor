//! `serde` helpers for reading Postman files, which are written by many tools
//! and are often untidy: fields missing, `null`, or the wrong type. A field
//! that cannot be understood reads as empty instead of failing the import.
//! The helpers here are used with `#[serde(default, deserialize_with = "...")]`.

use std::{fmt, marker::PhantomData};

use serde::{
    de::{value::MapAccessDeserializer, IgnoredAny, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer,
};
use serde_json::Value;

/// A string, or `None` for anything else (including `null`).
pub fn text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    Ok(match Value::deserialize(deserializer)? {
        Value::String(text) => Some(text),
        _ => None,
    })
}

/// A string, or `""` for anything else.
pub fn text_or_empty<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    text(deserializer).map(Option::unwrap_or_default)
}

/// Any scalar as text: strings as they are, `null` as `""`, numbers and
/// booleans as their JSON form. For variable values, which Postman files
/// sometimes hold as numbers.
pub fn stringified<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    Ok(match Value::deserialize(deserializer)? {
        Value::Null => String::new(),
        Value::String(text) => text,
        other => other.to_string(),
    })
}

/// An enum read from its string, `None` for anything else. A string the enum
/// does not name reads as its own fallback (see `stored_enum!`).
pub fn lenient_enum<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    Ok(serde_json::from_value(Value::deserialize(deserializer)?).ok())
}

/// A boolean, or `None` for anything else.
pub fn flag<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<bool>, D::Error> {
    Ok(match Value::deserialize(deserializer)? {
        Value::Bool(flag) => Some(flag),
        _ => None,
    })
}

/// A boolean, or `false` for anything else (`"disabled"` flags).
pub fn flag_or_false<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    flag(deserializer).map(|flag| flag.unwrap_or(false))
}

/// `null` reads as the type's default, for fields that are structs.
pub fn null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// A list, or an empty one when the field holds anything that is not a list
/// (for example Postman's `"header": "Accept: */*"` string form). Streams the
/// elements, so a large `item` array is not buffered first.
pub fn lenient_list<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ListVisitor<T>(PhantomData<T>);

    impl<'de, T: Deserialize<'de>> Visitor<'de> for ListVisitor<T> {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a list")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(1024));
            while let Some(item) = seq.next_element()? {
                items.push(item);
            }
            Ok(items)
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
            Ok(Vec::new())
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }

        fn visit_bool<E>(self, _: bool) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }

        fn visit_i64<E>(self, _: i64) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }

        fn visit_u64<E>(self, _: u64) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }

        fn visit_f64<E>(self, _: f64) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }

        fn visit_str<E>(self, _: &str) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }
    }

    deserializer.deserialize_any(ListVisitor(PhantomData))
}

/// Postman writes `request` and `url` either as a bare string or as an object.
pub trait FromText {
    fn from_text(text: &str) -> Self;
}

/// An object as `T`, a string through [`FromText`], `None` for `null`.
pub fn object_or_text<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + FromText,
{
    struct ObjectOrText<T>(PhantomData<T>);

    impl<'de, T: Deserialize<'de> + FromText> Visitor<'de> for ObjectOrText<T> {
        type Value = Option<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("an object or a string")
        }

        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
            T::deserialize(MapAccessDeserializer::new(map)).map(Some)
        }

        fn visit_str<E>(self, text: &str) -> Result<Self::Value, E> {
            Ok(Some(T::from_text(text)))
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
    }

    deserializer.deserialize_any(ObjectOrText(PhantomData))
}

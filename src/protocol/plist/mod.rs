//! Binary plist codec for `AirPlay` protocol messages
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(missing_docs)]
#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]

pub mod airplay;
pub mod decode;
pub mod encode;

pub use decode::{PlistDecodeError, decode};
pub use encode::{PlistEncodeError, encode};

use std::collections::HashMap;

/// A property list value
#[derive(Debug, Clone, PartialEq)]
pub enum PlistValue {
    /// Boolean value
    Boolean(bool),

    /// Unsigned integer (up to 64 bits)
    Integer(i64),

    /// Unsigned integer for large values
    UnsignedInteger(u64),

    /// Floating point number
    Real(f64),

    /// UTF-8 string
    String(String),

    /// Binary data
    Data(Vec<u8>),

    /// Date as seconds since 2001-01-01 00:00:00 UTC
    Date(f64),

    /// Array of values
    Array(Vec<PlistValue>),

    /// Dictionary (key-value pairs)
    Dictionary(HashMap<String, PlistValue>),

    /// UID reference (used internally)
    Uid(u64),
}

impl PlistValue {
    /// Try to get as boolean
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            PlistValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    /// Try to get as i64
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            PlistValue::Integer(i) => Some(*i),
            PlistValue::UnsignedInteger(u) => (*u).try_into().ok(),
            _ => None,
        }
    }

    /// Try to get as u64
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            PlistValue::Integer(i) => (*i).try_into().ok(),
            PlistValue::UnsignedInteger(u) => Some(*u),
            _ => None,
        }
    }

    /// Try to get as f64
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            PlistValue::Real(f) => Some(*f),
            #[allow(clippy::cast_precision_loss)]
            PlistValue::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// Try to get as string reference
    pub fn as_str(&self) -> Option<&str> {
        match self {
            PlistValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Try to get as byte slice
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            PlistValue::Data(d) => Some(d),
            _ => None,
        }
    }

    /// Try to get as array reference
    pub fn as_array(&self) -> Option<&[PlistValue]> {
        match self {
            PlistValue::Array(a) => Some(a),
            _ => None,
        }
    }

    /// Try to get as dictionary reference
    pub fn as_dict(&self) -> Option<&HashMap<String, PlistValue>> {
        match self {
            PlistValue::Dictionary(d) => Some(d),
            _ => None,
        }
    }

    /// Check if value is null/empty
    pub fn is_null(&self) -> bool {
        matches!(self, PlistValue::Data(d) if d.is_empty())
    }
}

impl From<bool> for PlistValue {
    fn from(v: bool) -> Self {
        PlistValue::Boolean(v)
    }
}

impl From<i32> for PlistValue {
    fn from(v: i32) -> Self {
        PlistValue::Integer(i64::from(v))
    }
}

impl From<i64> for PlistValue {
    fn from(v: i64) -> Self {
        PlistValue::Integer(v)
    }
}

impl From<u64> for PlistValue {
    fn from(v: u64) -> Self {
        PlistValue::UnsignedInteger(v)
    }
}

impl From<f64> for PlistValue {
    fn from(v: f64) -> Self {
        PlistValue::Real(v)
    }
}

impl From<String> for PlistValue {
    fn from(v: String) -> Self {
        PlistValue::String(v)
    }
}

impl From<&str> for PlistValue {
    fn from(v: &str) -> Self {
        PlistValue::String(v.to_string())
    }
}

impl From<Vec<u8>> for PlistValue {
    fn from(v: Vec<u8>) -> Self {
        PlistValue::Data(v)
    }
}

impl<T: Into<PlistValue>> From<Vec<T>> for PlistValue {
    fn from(v: Vec<T>) -> Self {
        PlistValue::Array(v.into_iter().map(Into::into).collect())
    }
}

impl<K: Into<String>, V: Into<PlistValue>> FromIterator<(K, V)> for PlistValue {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        PlistValue::Dictionary(
            iter.into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        )
    }
}

/// Builder for creating plist dictionaries
#[derive(Debug, Default)]
pub struct DictBuilder {
    map: HashMap<String, PlistValue>,
}

impl DictBuilder {
    /// Create a new dictionary builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a key-value pair
    pub fn insert(mut self, key: impl Into<String>, value: impl Into<PlistValue>) -> Self {
        self.map.insert(key.into(), value.into());
        self
    }

    /// Insert if value is Some
    pub fn insert_opt<V: Into<PlistValue>>(
        mut self,
        key: impl Into<String>,
        value: Option<V>,
    ) -> Self {
        if let Some(v) = value {
            self.map.insert(key.into(), v.into());
        }
        self
    }

    /// Build the dictionary
    pub fn build(self) -> PlistValue {
        PlistValue::Dictionary(self.map)
    }
}

/// Convenience macro for creating plist dictionaries
#[macro_export]
macro_rules! plist_dict {
    ($($key:expr => $value:expr),* $(,)?) => {
        $crate::protocol::plist::DictBuilder::new()
            $(.insert($key, $value))*
            .build()
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plist_value_accessors() {
        let value = PlistValue::Integer(42);
        assert_eq!(value.as_i64(), Some(42));
        assert_eq!(value.as_str(), None);
        assert_eq!(value.as_bool(), None);
    }

    #[test]
    fn test_plist_value_from_conversions() {
        assert!(matches!(PlistValue::from(true), PlistValue::Boolean(true)));
        assert!(matches!(PlistValue::from(42i64), PlistValue::Integer(42)));
        // Approximate float comparison
        match PlistValue::from(std::f64::consts::PI) {
            #[allow(clippy::approx_constant)]
            PlistValue::Real(f) => assert!((f - std::f64::consts::PI).abs() < f64::EPSILON),
            _ => panic!("Expected Real"),
        }

        match PlistValue::from("hello") {
            PlistValue::String(s) => assert_eq!(s, "hello"),
            _ => panic!("Expected String"),
        }
    }

    #[test]
    fn test_dict_builder() {
        let dict = DictBuilder::new()
            .insert("key1", "value1")
            .insert("key2", 42i64)
            .insert_opt("key3", Some("present"))
            .insert_opt::<String>("key4", None)
            .build();

        let d = dict.as_dict().unwrap();
        assert_eq!(d.len(), 3);
        assert!(d.contains_key("key1"));
        assert!(d.contains_key("key2"));
        assert!(d.contains_key("key3"));
        assert!(!d.contains_key("key4"));
    }

    #[test]
    fn test_plist_dict_macro() {
        let dict = plist_dict! {
            "name" => "test",
            "count" => 5i64,
        };

        let d = dict.as_dict().unwrap();
        assert_eq!(d.get("name").and_then(PlistValue::as_str), Some("test"));
        assert_eq!(d.get("count").and_then(PlistValue::as_i64), Some(5));
    }
}

//! Raw, untrusted intermediate representation of menu trees and property variants.

use std::collections::HashMap;

/// Untrusted property variant value.
#[derive(Debug, Clone, PartialEq)]
pub enum RawValue {
    /// String variant.
    String(String),
    /// Integer variant (i32, i64, u32, etc. normalized to i64).
    Int(i64),
    /// Boolean variant.
    Bool(bool),
    /// Array variant.
    Array(Vec<RawValue>),
    /// Map or dictionary variant represented as key-value pairs.
    Map(Vec<(String, RawValue)>),
    /// Raw byte array variant.
    Bytes(Vec<u8>),
    /// Unsupported or other variant type.
    Other,
}

impl RawValue {
    /// Returns the string slice if this variant is a string.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Returns the bool if this variant is a bool.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Returns the integer if this variant is an int.
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Returns the slice of child variants if this is an array.
    #[must_use]
    pub fn as_array(&self) -> Option<&[RawValue]> {
        match self {
            Self::Array(a) => Some(a.as_slice()),
            _ => None,
        }
    }

    /// Returns the map entries if this is a map.
    #[must_use]
    pub fn as_map(&self) -> Option<&[(String, RawValue)]> {
        match self {
            Self::Map(m) => Some(m.as_slice()),
            _ => None,
        }
    }
}

/// Raw untrusted node in a dbusmenu layout tree.
#[derive(Debug, Clone, PartialEq)]
pub struct RawMenuNode {
    /// Untrusted menu item identifier.
    pub id: i32,
    /// Untrusted property map.
    pub properties: HashMap<String, RawValue>,
    /// Untrusted list of child nodes.
    pub children: Vec<RawMenuNode>,
}

impl RawMenuNode {
    /// Creates a new raw node with the given ID.
    #[must_use]
    pub fn new(id: i32) -> Self {
        Self {
            id,
            properties: HashMap::new(),
            children: Vec::new(),
        }
    }

    /// Adds a property variant to this node.
    #[must_use]
    pub fn with_property(mut self, key: impl Into<String>, val: RawValue) -> Self {
        self.properties.insert(key.into(), val);
        self
    }

    /// Adds a child node to this node.
    #[must_use]
    pub fn with_child(mut self, child: RawMenuNode) -> Self {
        self.children.push(child);
        self
    }
}

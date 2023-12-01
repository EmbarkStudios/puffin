use std::{borrow::Cow, collections::HashSet, sync::Arc};

use parking_lot::RwLock;
use serde::{ser::SerializeStruct, Deserialize, Deserializer, Serialize, Serializer};

use crate::{clean_function_name, fetch_add_scope_id, short_file_name, ScopeId};

#[derive(Default, Clone)]
struct Inner {
    // Store a both-way map, memory wise this can be a bit redundant but allows for faster access of information by external libs.
    pub(crate) scope_id_to_details: std::collections::HashMap<ScopeId, ScopeDetails>,
    pub(crate) string_to_scope_id: std::collections::HashMap<String, ScopeId>,
}

/// Provides fast read access to scope details.
/// This collection can be cloned safely.
#[derive(Default, Clone)]
pub struct ScopeCollection(Arc<RwLock<Inner>>);

impl ScopeCollection {
    /// Provides read to the given closure for some scope details.
    pub fn read_by_id<F: FnMut(&ScopeDetails)>(&self, scope_id: &ScopeId, mut cb: F) {
        if let Some(read) = self.0.read().scope_id_to_details.get(scope_id) {
            cb(read);
        }
    }

    /// Provides read to the given closure for some scope details.
    pub fn read_by_name<F: FnMut(&ScopeId)>(&self, scope_name: &str, mut cb: F) {
        if let Some(read) = self.0.read().string_to_scope_id.get(scope_name) {
            cb(read);
        }
    }

    /// Only puffin should be allowed to allocate and provide the scope id so this function is private to puffin.
    pub(crate) fn insert(&self, scope_details: ScopeDetails) {
        assert!(scope_details.scope_id.is_some());

        let id = scope_details.identifier();
        self.0
            .write()
            .string_to_scope_id
            .insert(id.to_string(), scope_details.scope_id.unwrap());
        self.0.write().scope_id_to_details.insert(
            scope_details.scope_id.unwrap(),
            scope_details.into_readable(),
        );
    }

    /// Manually register scope details. After a scope is inserted it can be reported to puffin.
    pub fn register_custom_scopes(&self, scopes: &[ScopeDetails]) -> HashSet<ScopeId> {
        let mut new_scopes = HashSet::new();
        for scope_detail in scopes {
            let new_scope_id = fetch_add_scope_id();
            self.insert(scope_detail.clone().with_scope_id(new_scope_id));
            new_scopes.insert(new_scope_id);
        }

        new_scopes
    }

    /// Fetches all registered scopes and their ids.
    /// Useful for fetching scope id by a static scope name.
    pub fn scopes_by_name<T>(
        &self,
        mut existing_scopes: impl FnMut(&std::collections::HashMap<String, ScopeId>) -> T,
    ) -> T {
        existing_scopes(&self.0.read().string_to_scope_id)
    }

    /// Fetches all registered scopes.
    /// Useful for fetching scope details by a scope id.
    pub fn scopes_by_id<T>(
        &self,
        mut existing_scopes: impl FnMut(&std::collections::HashMap<ScopeId, ScopeDetails>) -> T,
    ) -> T {
        existing_scopes(&self.0.read().scope_id_to_details)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Hash, PartialOrd, Ord, Eq)]
/// This struct contains scope details and can be read by external libraries.
pub struct ScopeDetails {
    /// Unique scope Identifier.
    // Always initialized once registered.
    // It is `None` when an external library has yet to register this scope.
    pub(crate) scope_id: Option<ScopeId>,
    /// Identifier for a scope, for a function this is just the raw function name.
    pub scope_name: Option<Cow<'static, str>>,
    /// The function name of the function in which this scope is contained.
    /// The name might be slightly modified to represent a short descriptive name.
    pub function_name: Cow<'static, str>,
    /// The file path in which this scope is contained.
    /// The path might be slightly modified to represent a short descriptive name.
    pub file_path: Cow<'static, str>,
    /// The exact line number at which this scope is located.
    pub line_nr: u32,
}

impl ScopeDetails {
    /// Create a new custom scope with a unique name.
    pub fn from_scope_name<T>(scope_name: T) -> Self
    where
        T: Into<Cow<'static, str>>,
    {
        Self {
            scope_id: None,
            scope_name: Some(scope_name.into()),
            function_name: Default::default(),
            file_path: Default::default(),
            line_nr: Default::default(),
        }
    }

    /// Create a new custom scope with a unique id allocated by puffin.
    /// This function should not be exposed as only puffin should allocate ids.
    pub(crate) fn from_scope_id(scope_id: ScopeId) -> Self {
        Self {
            scope_id: Some(scope_id),
            scope_name: None,
            function_name: Default::default(),
            file_path: Default::default(),
            line_nr: Default::default(),
        }
    }

    #[inline]
    pub fn with_function_name<T>(mut self, name: T) -> Self
    where
        T: Into<Cow<'static, str>>,
    {
        self.function_name = name.into();
        self
    }

    #[inline]
    pub fn with_file<T>(mut self, file: T) -> Self
    where
        T: Into<Cow<'static, str>>,
    {
        self.file_path = file.into();
        self
    }

    #[inline]
    pub fn with_line_nr(mut self, line_nr: u32) -> Self {
        self.line_nr = line_nr;
        self
    }

    // Scopes are identified by user-provided name while functions are identified by the function name.
    pub fn identifier(&self) -> &'_ Cow<'static, str> {
        self.scope_name
            .as_ref()
            .unwrap_or_else(|| &self.function_name)
    }

    #[inline]
    pub fn location(&self) -> String {
        format!("{}:{}", self.file_path, self.line_nr)
    }

    #[inline]
    pub(crate) fn into_readable(self) -> Self {
        Self {
            scope_id: self.scope_id,
            scope_name: self.scope_name,
            function_name: clean_function_name(&self.function_name).into(),
            file_path: short_file_name(&self.file_path).into(),
            line_nr: self.line_nr,
        }
    }

    #[inline]
    pub(crate) fn with_scope_id(mut self, scope_id: ScopeId) -> Self {
        self.scope_id = Some(scope_id);
        self
    }

    #[inline]
    pub(crate) fn with_scope_name<T>(mut self, scope_name: T) -> Self
    where
        T: Into<Cow<'static, str>>,
    {
        self.scope_name = Some(scope_name.into());
        self
    }
}

impl Serialize for ScopeDetails {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct(stringify!(ScopeDetails), 5)?;
        state.serialize_field("scope_id", &self.scope_id)?;
        state.serialize_field("scope_name", &self.scope_name)?;
        state.serialize_field("function_name", &self.function_name)?;
        state.serialize_field("file_path", &self.file_path)?;
        state.serialize_field("line_nr", &self.line_nr)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for ScopeDetails {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ScopeDetailsOwnedVisitor;

        impl<'de> serde::de::Visitor<'de> for ScopeDetailsOwnedVisitor {
            type Value = ScopeDetails;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&format!("struct {}", stringify!(ScopeDetails)))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let scope_id = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::missing_field("scope_id"))?;
                let scope_name = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::missing_field("scope_name"))?;
                let function_name = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::missing_field("function_name"))?;
                let file_path = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::missing_field("file_path"))?;
                let line_nr = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::missing_field("line_nr"))?;

                Ok(ScopeDetails {
                    scope_id: Some(scope_id),
                    scope_name,
                    function_name,
                    file_path,
                    line_nr,
                })
            }
        }

        const FIELDS: &[&str] = &[
            "scope_id",
            "scope_name",
            "function_name",
            "file_path",
            "line_nr",
        ];
        deserializer.deserialize_struct(stringify!(ScopeDetails), FIELDS, ScopeDetailsOwnedVisitor)
    }
}

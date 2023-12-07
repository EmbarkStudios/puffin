use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

use once_cell::sync::Lazy;
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use serde::{ser::SerializeStruct, Deserialize, Deserializer, Serialize, Serializer};

use crate::{clean_function_name, fetch_add_scope_id, short_file_name, ScopeId};

// Scope details are stored separately from [`GlobalProfiler`] for the following reasons:
//
// 1. Almost every access only requires read access so there is no need for mutex lock the global profiler.
// 2. Its important to guarantee that the profiler path is lock-free.
// 2. External libraries like the http server or ui require read/write access to scopes.
// But that can easily end up in deadlocks if a profile scope is executed while scope details are being read.
// Storing the scope collection outside the [`GlobalProfiler`] prevents deadlocks.
static SCOPE_COLLECTION: Lazy<RwLock<ScopeCollection>> = Lazy::new(Default::default);

#[derive(Default, Clone)]
struct Inner {
    // Store a both-way map, memory wise this can be a bit redundant but allows for faster access of information by external libs.
    pub(crate) scope_id_to_details: HashMap<ScopeId, ScopeDetails>,
    pub(crate) string_to_scope_id: HashMap<String, ScopeId>,
}

/// Provides fast read access to scope details.
/// This collection can be cloned safely.
#[derive(Default, Clone)]
pub struct ScopeCollection(Inner);

impl ScopeCollection {
    /// Fetches the scope collection with details for each scope.
    pub fn instance<'a>() -> RwLockReadGuard<'a, ScopeCollection> {
        SCOPE_COLLECTION.read()
    }

    /// Fetches mutable access to the scope collection with details for each scope.
    /// This should only be used if you know what your doing.
    /// Scope details are automatically registered when using the profile macros.
    /// Use [`Self::insert_custom_scopes`] for inserting custom scopes.
    pub fn instance_mut<'a>() -> RwLockWriteGuard<'a, ScopeCollection> {
        SCOPE_COLLECTION.write()
    }

    /// Fetches scope details by scope id.
    pub fn read_by_id(&self, scope_id: &ScopeId) -> Option<&ScopeDetails> {
        self.0.scope_id_to_details.get(scope_id)
    }

    /// Fetches scope details by scope name.
    pub fn read_by_name(&self, scope_name: &str) -> Option<&ScopeId> {
        self.0.string_to_scope_id.get(scope_name)
    }

    /// Only puffin should be allowed to allocate and provide the scope id so this function is private to puffin.
    pub(crate) fn insert(&mut self, scope_details: ScopeDetails) {
        assert!(scope_details.scope_id.is_some());

        let id = scope_details.identifier();

        self.0
            .string_to_scope_id
            .insert(id.to_string(), scope_details.scope_id.unwrap());
        self.0.scope_id_to_details.insert(
            scope_details.scope_id.unwrap(),
            scope_details.into_readable(),
        );
    }

    /// Manually register scope details. After a scope is inserted it can be reported to puffin.
    pub fn register_custom_scopes(&mut self, scopes: &[ScopeDetails]) -> HashSet<ScopeId> {
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
        existing_scopes(&self.0.string_to_scope_id)
    }

    /// Fetches all registered scopes.
    /// Useful for fetching scope details by a scope id.
    pub fn scopes_by_id<T>(
        &self,
        mut existing_scopes: impl FnMut(&std::collections::HashMap<ScopeId, ScopeDetails>) -> T,
    ) -> T {
        existing_scopes(&self.0.scope_id_to_details)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Hash, PartialOrd, Ord, Eq)]
/// Detailed information about a scope.
pub struct ScopeDetails {
    /// Unique scope identifier.
    /// Always initialized once registered.
    /// It is `None` when an external library has yet to register this scope.
    pub(crate) scope_id: Option<ScopeId>,
    /// A name for a profile scope, a function profile scope does not have a custom provided name.
    pub scope_name: Option<Cow<'static, str>>,
    /// The function name of the function in which this scope is contained.
    /// The name might be slightly modified to represent a short descriptive representation.
    pub function_name: Cow<'static, str>,
    /// The file path in which this scope is contained.
    /// The path might be slightly modified to represent a short descriptive representation.
    pub file_path: Cow<'static, str>,
    /// The exact line number at which this scope is located.
    pub line_nr: u32,
}

impl ScopeDetails {
    /// Creates a new custom scope with a unique name.
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

    /// Creates a new custom scope with a unique id allocated by puffin.
    /// This function should not be exposed as only puffin should allocate ids for scopes.
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
        self.scope_name.as_ref().unwrap_or(&self.function_name)
    }

    /// Returns the exact location of the profile scope formatted as `file:line_nr`
    #[inline]
    pub fn location(&self) -> String {
        format!("{}:{}", self.file_path, self.line_nr)
    }

    /// Turns the scope details into a more readable version:
    ///
    /// * Consistent / shortened file path across platforms
    /// * Consistent / shortened function name
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

    // This function should not be exposed as only puffin should allocate ids.
    #[inline]
    pub(crate) fn with_scope_id(mut self, scope_id: ScopeId) -> Self {
        self.scope_id = Some(scope_id);
        self
    }

    // This function should not be exposed as users are supposed to provide scope name in constructor.
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
                    scope_id,
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

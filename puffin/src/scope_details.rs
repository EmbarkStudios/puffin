use std::{borrow::Cow, collections::HashSet, sync::Arc};

use parking_lot::RwLock;
use serde::{ser::SerializeStruct, Deserialize, Deserializer, Serialize, Serializer};

use crate::{clean_function_name, fetch_add_scope_id, short_file_name, ScopeId};

#[derive(Default, Clone)]
struct Inner {
    pub(crate) scope_id_to_details: std::collections::HashMap<ScopeId, ScopeDetails>,
    pub(crate) string_to_scope_id: std::collections::HashMap<String, ScopeId>,
}

/// Provides fast read access to scope details.
/// This collection can be cloned safely.
#[derive(Default, Clone)]
pub struct ScopeCollection(
    // Store a both-way map, memory wise this can be a bit redundant but allows for faster access of information.
    Arc<RwLock<Inner>>,
);

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
    pub(crate) fn insert_with_id(&self, scope_id: ScopeId, scope_details: ScopeDetails) {
        self.0
            .write()
            .string_to_scope_id
            .insert(scope_details.scope_name.to_string(), scope_id);
        self.0
            .write()
            .scope_id_to_details
            .insert(scope_id, scope_details);
    }

    /// Manually register scope details. After a scope is inserted it can be reported to puffin.
    pub fn register_custom_scopes(&self, scopes: &[ScopeDetails]) -> HashSet<ScopeId> {
        let mut new_scopes = HashSet::new();
        for scope_detail in scopes {
            let new_scope_id = fetch_add_scope_id();
            self.insert_with_id(new_scope_id, scope_detail.clone());
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

/// Scope details that are registered by the macros.
/// This is only used internally and should not be exposed.
#[derive(Debug, Clone, Copy, PartialEq, Hash, PartialOrd, Ord, Eq)]
#[doc(hidden)]
pub struct ScopeDetailsStatic {
    /// Identifier for the scope being registered.
    pub(crate) scope_id: ScopeId,
    // Custom provided static id that identifiers a scope in a function.
    pub(crate) scope_name: &'static str,
    /// Scope name, or function name.
    pub(crate) function_name: &'static str,
    /// Path to the file containing the profiling macro
    pub(crate) file: &'static str,
    /// The line number containing the profiling
    pub(crate) line_nr: u32,
}
#[derive(Debug, Default, Clone, PartialEq, Hash, PartialOrd, Ord, Eq)]
/// This struct contains scope details and can be read by external libraries.
pub struct ScopeDetails {
    // Always initialized once registered.
    // Its only none when a external library has yet to register this scope.
    pub(crate) scope_id: Option<ScopeId>,
    /// Identifier for a scope, for a function this is just the raw function name.
    pub scope_name: Cow<'static, str>,
    /// Shorter variant of the raw function name.
    /// This is more descriptive and shorter.
    /// In the case of custom scopes provided scopes this contains the `scope name`.
    pub function_name: Cow<'static, str>,
    /// Shorter variant of the raw file path.
    /// This is cleaned up and made consistent across platforms.
    /// In the case of custom scopes provided scopes this contains the `file name`.
    pub file_path: Cow<'static, str>,
    /// The line number containing the profiling
    pub line_nr: u32,
    /// File name plus line number: path:number
    pub location: String,
}

impl ScopeDetails {
    /// Create a new custom scope with a unique name.
    pub fn from_scope_name(scope_name: impl Into<Cow<'static, str>>) -> Self {
        Self {
            scope_id: None,
            scope_name: scope_name.into(),
            function_name: Default::default(),
            file_path: Default::default(),
            line_nr: Default::default(),
            location: Default::default(),
        }
    }

    /// Create a new custom scope with a unique name.
    fn from_scope_id(scope_id: ScopeId) -> Self {
        Self {
            scope_id: Some(scope_id),
            scope_name: Default::default(),
            function_name: Default::default(),
            file_path: Default::default(),
            line_nr: Default::default(),
            location: Default::default(),
        }
    }

    #[inline]
    pub fn with_function_name(mut self, name: impl Into<Cow<'static, str>>) -> Self {
        self.function_name = name.into();
        self
    }

    #[inline]
    pub fn with_file(mut self, file: Cow<'static, str>) -> Self {
        self.file_path = file;
        self.update_location();
        self
    }

    #[inline]
    pub fn with_line_nr(mut self, line_nr: u32) -> Self {
        self.line_nr = line_nr;
        self.update_location();
        self
    }

    #[inline]
    fn update_location(&mut self) {
        self.location = format!("{}:{}", self.file_path, self.line_nr)
    }
}

impl From<ScopeDetailsStatic> for ScopeDetails {
    fn from(value: ScopeDetailsStatic) -> Self {
        let cleaned_function_name = clean_function_name(value.function_name);

        ScopeDetails::from_scope_id(value.scope_id)
            .with_file(Cow::Owned(short_file_name(value.file)))
            .with_function_name(Cow::Owned(cleaned_function_name))
            .with_line_nr(value.line_nr)
    }
}

impl Serialize for ScopeDetails {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("ScopeDetails", 6)?;
        state.serialize_field("scope_id", &self.scope_id)?;
        state.serialize_field("scope_name", &self.scope_name)?;
        state.serialize_field("function_name", &self.function_name)?;
        state.serialize_field("file_path", &self.file_path)?;
        state.serialize_field("line_nr", &self.line_nr)?;
        state.serialize_field("location", &self.location)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for ScopeDetails {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            ScopeId,
            ScopeName,
            DynamicFunctionName,
            DynamicFilePath,
            LineNr,
            Location,
        }

        struct ScopeDetailsOwnedVisitor;

        impl<'de> serde::de::Visitor<'de> for ScopeDetailsOwnedVisitor {
            type Value = ScopeDetails;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct ScopeDetailsOwned")
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
                let location = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::missing_field("location"))?;

                Ok(ScopeDetails {
                    scope_id: Some(scope_id),
                    scope_name,
                    function_name,
                    file_path,
                    line_nr,
                    location,
                })
            }
        }

        const FIELDS: &[&str] = &[
            "scope_id",
            "scope_name",
            "function_name",
            "file_path",
            "line_nr",
            "location",
        ];
        deserializer.deserialize_struct("ScopeDetails", FIELDS, ScopeDetailsOwnedVisitor)
    }
}

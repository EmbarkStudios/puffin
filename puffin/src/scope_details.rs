use std::{borrow::Cow, collections::HashSet, sync::Arc};

use parking_lot::RwLock;

use crate::{clean_function_name, fetch_add_scope_id, short_file_name, ScopeId};

#[derive(Default, Clone)]
struct InnerDetails {
    pub(crate) scope_id_to_details: std::collections::HashMap<ScopeId, ScopeDetailsOwned>,
    pub(crate) string_to_scope_id: std::collections::HashMap<String, ScopeId>,
}

/// Provides read access to scope details.
#[derive(Default, Clone)]
pub struct ScopeDetails(
    // Store a both-way map, memory wise this can be a bit redundant but allows for faster access of information.
    Arc<RwLock<InnerDetails>>,
);

impl ScopeDetails {
    /// Provides read to the given closure for some scope details.
    pub fn read_by_id<F: FnMut(&ScopeDetailsOwned)>(&self, scope_id: &ScopeId, mut cb: F) {
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

    /// Function is hidden as only puffin library will insert entries.
    pub(crate) fn insert(&self, scope_id: ScopeId, scope_details: ScopeDetailsOwned) {
        self.0
            .write()
            .string_to_scope_id
            .insert(scope_details.scope_name.to_string(), scope_id);
        self.0
            .write()
            .scope_id_to_details
            .insert(scope_id, scope_details);
    }

    /// Manually insert scope details. After a scope is inserted it can be reported to puffin.
    pub fn insert_custom_scopes(&self, scopes: &[CustomScopeDetails]) -> HashSet<ScopeId> {
        let mut new_scopes = HashSet::new();
        for scope_detail in scopes {
            let new_scope_id = fetch_add_scope_id();

            self.insert(new_scope_id, ScopeDetailsOwned::from(scope_detail));

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
        mut existing_scopes: impl FnMut(&std::collections::HashMap<ScopeId, ScopeDetailsOwned>) -> T,
    ) -> T {
        existing_scopes(&self.0.read().scope_id_to_details)
    }
}

/// Scope details that are registered by the macros.
/// This is only used internally and should not be exposed.
#[derive(Debug, Clone, Copy, PartialEq, Hash, PartialOrd, Ord, Eq)]
pub struct ScopeDetailsRef {
    /// Identifier for the scope being registered.
    pub scope_id: ScopeId,
    // Custom provided static id that identifiers a scope in a function.
    pub scope_name: &'static str,
    /// Scope name, or function name (previously called "id")
    pub raw_function_name: &'static str,
    /// Path to the file containing the profiling macro
    pub file: &'static str,
    /// The line number containing the profiling
    pub line_nr: u32,
}

/// Custom scope details that can be registered by external users.
/// Instantiate this type once for each custom scope that you record manually.
/// Custom scopes can be registered via `GlobalProfiler::scope_details().insert_scopes()`.
// This type provides slightly more convenient api for external users.
#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, Ord, Eq)]
pub struct CustomScopeDetails {
    /// Unique identifier for this scope.
    pub scope_name: Cow<'static, str>,
    /// Scope name, or function name (previously called "id")
    pub function_name: Cow<'static, str>,
    /// Path to the file containing the profiling macro
    /// Can be empty if unused.
    pub file_name: Cow<'static, str>,
    /// The line number containing the profiling.
    /// Can be 0 if unused.
    pub line_nr: u32,
}

impl CustomScopeDetails {
    /// Create a new custom scope with a unique name.
    pub fn new(scope_name: impl Into<Cow<'static, str>>) -> Self {
        Self {
            scope_name: scope_name.into(),
            function_name: Default::default(),
            file_name: Default::default(),
            line_nr: Default::default(),
        }
    }

    pub fn with_function_name(mut self, name: impl Into<Cow<'static, str>>) -> Self {
        self.function_name = name.into();
        self
    }

    pub fn with_file(mut self, file: Cow<'static, str>) -> Self {
        self.file_name = file;
        self
    }

    pub fn with_line_nr(mut self, line_nr: u32) -> Self {
        self.line_nr = line_nr;
        self
    }
}

#[derive(Debug, Default, Clone, PartialEq, Hash, PartialOrd, Ord, Eq)]
/// This struct contains scope details and can be read by external libraries.
pub struct ScopeDetailsOwned {
    pub scope_name: Cow<'static, str>,
    /// Shorter variant of the raw function name.
    /// This is more descriptive and shorter.
    /// In the case of custom scopes provided scopes this contains the `scope name`.
    pub dynamic_function_name: Cow<'static, str>,
    /// Raw function name with the entire type path.
    /// This is empty for custom scopes.
    pub raw_function_name: &'static str,
    /// Shorter variant of the raw file path.
    /// This is cleaned up and made consistent across platforms.
    /// In the case of custom scopes provided scopes this contains the `file name`.
    pub dynamic_file_path: Cow<'static, str>,
    /// The full raw file path to the file in which the scope was located.
    /// This is empty for custom scopes.
    pub raw_file_path: &'static str,
    /// The line number containing the profiling
    pub line_nr: u32,
    /// File name plus line number: path:number
    pub location: String,
}

impl From<ScopeDetailsRef> for ScopeDetailsOwned {
    fn from(value: ScopeDetailsRef) -> Self {
        let cleaned_function_name = clean_function_name(value.raw_function_name);

        ScopeDetailsOwned {
            scope_name: format!("{cleaned_function_name}:{}", value.line_nr).into(),
            location: format!("{cleaned_function_name}:{}", value.line_nr),
            raw_function_name: value.raw_function_name,
            dynamic_function_name: Cow::Owned(cleaned_function_name),
            raw_file_path: value.file,
            dynamic_file_path: Cow::Owned(short_file_name(value.file)),
            line_nr: value.line_nr,
        }
    }
}

impl From<&CustomScopeDetails> for ScopeDetailsOwned {
    fn from(value: &CustomScopeDetails) -> Self {
        let mut location = String::new();

        if !value.file_name.is_empty() {
            location = format!("{}:{}", value.file_name, value.line_nr);
        }

        ScopeDetailsOwned {
            scope_name: value.scope_name.clone(),
            dynamic_function_name: value.function_name.clone(),
            raw_function_name: "-", // user provided scopes are non-static
            dynamic_file_path: value.file_name.clone(),
            raw_file_path: "-", // user provided scopes are non-static
            line_nr: 0,
            location,
        }
    }
}

/// Scope details that can be serialized.
#[cfg_attr(
    feature = "serialization",
    derive(serde::Serialize, serde::Deserialize)
)]
pub(crate) struct SerdeScopeDetails {
    pub scope_id: ScopeId,
    pub scope_name: String,
    pub function_name: String,
    pub file_path: String,
    pub line_nmr: u32,
}

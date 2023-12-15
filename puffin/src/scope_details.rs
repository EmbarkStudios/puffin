use crate::{clean_function_name, fetch_add_scope_id, short_file_name, ScopeId};
use serde::{Deserialize, Serialize};
use std::ops::Deref;
use std::{borrow::Cow, collections::HashMap, sync::Arc};

#[derive(Default, Clone)]
struct Inner {
    // Store a both-way map, memory wise this can be a bit redundant but allows for faster access of information by external libs.
    pub(crate) scope_id_to_details: HashMap<ScopeId, Arc<ScopeDetails>>,
    pub(crate) type_to_scope_id: HashMap<ScopeType, ScopeId>,
}

/// A collection of scope details containing more information about a recorded profile scope.
#[derive(Default, Clone)]
pub struct ScopeCollection(Inner);

impl ScopeCollection {
    /// Fetches scope details by scope id.
    #[inline]
    pub fn fetch_by_id(&self, scope_id: &ScopeId) -> Option<&Arc<ScopeDetails>> {
        self.0.scope_id_to_details.get(scope_id)
    }

    /// Fetches scope details by scope name.
    #[inline]
    pub fn fetch_by_name(&self, scope_type: &ScopeType) -> Option<&ScopeId> {
        self.0.type_to_scope_id.get(scope_type)
    }

    /// Insert a scope into the collection.
    /// Note that only puffin should allocate and provide the scope id.
    /// But there might be instances like in http-client were one needs to insert a scope manually with the scope id set by the server.
    pub fn insert(&mut self, scope_details: Arc<ScopeDetails>) -> Arc<ScopeDetails> {
        assert!(scope_details.scope_id.is_some());

        let scope_type = scope_details.scope_type();

        self.0
            .type_to_scope_id
            .insert(scope_type, scope_details.scope_id.unwrap());
        self.0
            .scope_id_to_details
            .entry(scope_details.scope_id.unwrap())
            .or_insert(Arc::new(scope_details.deref().clone().into_readable()))
            .clone()
    }

    /// Manually register scope details. After a scope is inserted it can be reported to puffin.
    pub(crate) fn register_user_scopes(
        &mut self,
        scopes: &[ScopeDetails],
    ) -> Vec<Arc<ScopeDetails>> {
        let mut new_scopes = Vec::new();
        for scope_detail in scopes {
            let new_scope_id = fetch_add_scope_id();
            let scope = self.insert(Arc::new(
                (*scope_detail)
                    .clone()
                    .with_scope_id(new_scope_id)
                    .into_readable(),
            ));
            new_scopes.push(scope);
        }
        new_scopes
    }

    /// Fetches all registered scopes and their ids.
    /// Useful for fetching scope id by a static scope name.
    /// For profiler scopes and user scopes this is the manual provided name.
    /// For function profiler scopes this is the function name.
    #[inline]
    pub fn scopes_by_name(&self) -> &HashMap<ScopeType, ScopeId> {
        &self.0.type_to_scope_id
    }

    /// Fetches all registered scopes.
    /// Useful for fetching scope details by a scope id.
    #[inline]
    pub fn scopes_by_id(&self) -> &HashMap<ScopeId, Arc<ScopeDetails>> {
        &self.0.scope_id_to_details
    }
}

// Scopes are identified by user-provided name while functions are identified by the function name.
#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, Ord, Eq, Serialize, Deserialize)]
pub enum ScopeType {
    /// The scope is a function profile scope identified by the name of this function.
    Function(Cow<'static, str>),
    /// The scope is a profile scope inside a function identified by the name of this scope.
    Scope(Cow<'static, str>),
}

impl ScopeType {
    pub fn name<'a>(&self) -> &'a Cow<'static, str> {
        match self {
            ScopeType::Function(name) | ScopeType::Scope(name) => name,
        }
    }
}

impl ScopeType {
    pub fn function_scope<T>(function_name: T) -> Self
    where
        T: Into<Cow<'static, str>>,
    {
        Self::Function(function_name.into())
    }

    pub fn scope<T>(scope_name: T) -> Self
    where
        T: Into<Cow<'static, str>>,
    {
        Self::Scope(scope_name.into())
    }
}

#[derive(Debug, Default, Clone, PartialEq, Hash, PartialOrd, Ord, Eq, Serialize, Deserialize)]
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
    /// Creates a new user scope with a unique name.
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

    /// Creates a new user scope with a unique id allocated by puffin.
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

    pub fn scope_type(&self) -> ScopeType {
        self.scope_name
            .as_ref()
            .map(|x| ScopeType::scope(x.clone()))
            .unwrap_or(ScopeType::function_scope(self.function_name.clone()))
    }

    /// Returns the exact location of the profile scope formatted as `file:line_nr`
    #[inline]
    pub fn location(&self) -> String {
        if self.line_nr != 0 {
            format!("{}:{}", self.file_path, self.line_nr)
        } else {
            format!("{}", self.file_path)
        }
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

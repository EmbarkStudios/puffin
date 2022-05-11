//! A layer to integrate puffin as a tracing subscriber.
//!
//! ```
//! use puffin_tracing::PuffinLayer;
//! use tracing::info_span;
//! use tracing_subscriber::{layer::SubscriberExt, Registry};
//!
//! fn main() {
//!     let subscriber = Registry::default().with(PuffinLayer::new());
//!     tracing::subscriber::set_global_default(subscriber).unwrap();
//!
//!     puffin::set_scopes_on(true);
//!
//!     // ...
//! }
//!
//! fn my_function() {
//!     let _span = info_span!("My Function");
//! }
//! ```

// BEGIN - Embark standard lints v5 for Rust 1.55+
// do not change or add/remove here, but one can add exceptions after this section
// for more info see: <https://github.com/EmbarkStudios/rust-ecosystem/issues/59>
#![deny(unsafe_code)]
#![warn(
    clippy::all,
    clippy::await_holding_lock,
    clippy::char_lit_as_u8,
    clippy::checked_conversions,
    clippy::dbg_macro,
    clippy::debug_assert_with_mut_call,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::doc_markdown,
    clippy::empty_enum,
    clippy::enum_glob_use,
    clippy::exit,
    clippy::expl_impl_clone_on_copy,
    clippy::explicit_deref_methods,
    clippy::explicit_into_iter_loop,
    clippy::fallible_impl_from,
    clippy::filter_map_next,
    clippy::flat_map_option,
    clippy::float_cmp_const,
    clippy::fn_params_excessive_bools,
    clippy::from_iter_instead_of_collect,
    clippy::if_let_mutex,
    clippy::implicit_clone,
    clippy::imprecise_flops,
    clippy::inefficient_to_string,
    clippy::invalid_upcast_comparisons,
    clippy::large_digit_groups,
    clippy::large_stack_arrays,
    clippy::large_types_passed_by_value,
    clippy::let_unit_value,
    clippy::linkedlist,
    clippy::lossy_float_literal,
    clippy::macro_use_imports,
    clippy::manual_ok_or,
    clippy::map_err_ignore,
    clippy::map_flatten,
    clippy::map_unwrap_or,
    clippy::match_on_vec_items,
    clippy::match_same_arms,
    clippy::match_wild_err_arm,
    clippy::match_wildcard_for_single_variants,
    clippy::mem_forget,
    clippy::mismatched_target_os,
    clippy::missing_enforced_import_renames,
    clippy::mut_mut,
    clippy::mutex_integer,
    clippy::needless_borrow,
    clippy::needless_continue,
    clippy::needless_for_each,
    clippy::option_option,
    clippy::path_buf_push_overwrite,
    clippy::ptr_as_ptr,
    clippy::rc_mutex,
    clippy::ref_option_ref,
    clippy::rest_pat_in_fully_bound_structs,
    clippy::same_functions_in_if_condition,
    clippy::semicolon_if_nothing_returned,
    clippy::single_match_else,
    clippy::string_add_assign,
    clippy::string_add,
    clippy::string_lit_as_bytes,
    clippy::string_to_string,
    clippy::todo,
    clippy::trait_duplication_in_bounds,
    clippy::unimplemented,
    clippy::unnested_or_patterns,
    clippy::unused_self,
    clippy::useless_transmute,
    clippy::verbose_file_reads,
    clippy::zero_sized_map_values,
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms
)]
// crate-specific exceptions:
#![deny(missing_docs)]

use puffin::ThreadProfiler;
use std::{cell::RefCell, collections::VecDeque};
use tracing_core::{
    span::{Attributes, Id, Record},
    Subscriber,
};
use tracing_subscriber::{
    fmt::{format::DefaultFields, FormatFields, FormattedFields},
    layer::Context,
    registry::LookupSpan,
    Layer,
};

thread_local! {
    static PUFFIN_SPAN_STACK: RefCell<VecDeque<(Id, usize)>> =
        RefCell::new(VecDeque::with_capacity(16));
}

/// A tracing layer that collects data for puffin.
pub struct PuffinLayer<F = DefaultFields> {
    fmt: F,
}

impl Default for PuffinLayer<DefaultFields> {
    fn default() -> Self {
        Self {
            fmt: DefaultFields::default(),
        }
    }
}

impl PuffinLayer<DefaultFields> {
    /// Create a new `PuffinLayer`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Use a custom field formatting implementation.
    pub fn with_formatter<F>(self, fmt: F) -> PuffinLayer<F> {
        let _ = self;
        PuffinLayer { fmt }
    }
}

impl<S, F> Layer<S> for PuffinLayer<F>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    F: for<'writer> FormatFields<'writer> + 'static,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if !puffin::are_scopes_on() {
            return;
        }

        if let Some(span) = ctx.span(id) {
            let mut extensions = span.extensions_mut();
            if extensions.get_mut::<FormattedFields<F>>().is_none() {
                let mut fields = FormattedFields::<F>::new(String::with_capacity(64));
                if self.fmt.format_fields(fields.as_writer(), attrs).is_ok() {
                    extensions.insert(fields);
                }
            }
        }
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            let mut extensions = span.extensions_mut();
            if let Some(fields) = extensions.get_mut::<FormattedFields<F>>() {
                let _ = self.fmt.add_fields(fields, values);
            } else {
                let mut fields = FormattedFields::<F>::new(String::with_capacity(64));
                if self.fmt.format_fields(fields.as_writer(), values).is_ok() {
                    extensions.insert(fields);
                }
            }
        }
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        if !puffin::are_scopes_on() {
            return;
        }

        if let Some(span_data) = ctx.span(id) {
            let metadata = span_data.metadata();
            let name = metadata.name();
            let target = metadata.target();
            let extensions = span_data.extensions();
            let data = extensions
                .get::<FormattedFields<F>>()
                .map(|fields| fields.fields.as_str())
                .unwrap_or_default();

            ThreadProfiler::call(|tp| {
                let start_stream_offset = tp.begin_scope(name, target, data);
                PUFFIN_SPAN_STACK.with(|s| {
                    s.borrow_mut().push_back((id.clone(), start_stream_offset));
                });
            });
        }
    }

    fn on_exit(&self, id: &Id, _ctx: Context<'_, S>) {
        PUFFIN_SPAN_STACK.with(|s| {
            let value = s.borrow_mut().pop_back();
            if let Some((last_id, start_stream_offset)) = value {
                if *id == last_id {
                    ThreadProfiler::call(|tp| tp.end_scope(start_stream_offset));
                } else {
                    s.borrow_mut().push_back((last_id, start_stream_offset));
                }
            }
        });
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(&id) {
            span.extensions_mut().remove::<FormattedFields<F>>();
        }
    }
}

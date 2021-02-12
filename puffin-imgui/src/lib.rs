//! An [imgui-rs](https://crates.io/crates/imgui-rs) interface for [`puffin`].
//!
//! Usage:
//!
//! ``` ignored
//! fn main() {
//!     puffin::set_scopes_on(true); // you may want to control this with a flag
//!     let mut puffin_ui = puffin_imgui::ProfilerUi::default();
//!
//!     // game loop
//!     loop {
//!         puffin::GlobalProfiler::lock().new_frame();
//!
//!         {
//!             puffin::profile_scope!("slow_code");
//!             slow_code();
//!         }
//!
//!         puffin_ui.window(ui);
//!     }
//! }
//!
//! # fn slow_code(){}
//! ```

#![forbid(unsafe_code)]
#![warn(
    clippy::all,
    clippy::doc_markdown,
    clippy::dbg_macro,
    clippy::todo,
    clippy::empty_enum,
    clippy::enum_glob_use,
    clippy::pub_enum_variant_names,
    clippy::mem_forget,
    //clippy::use_self,   // disabled for now due to https://github.com/rust-lang/rust-clippy/issues/3410
    clippy::filter_map_next,
    clippy::needless_continue,
    clippy::needless_borrow,
    clippy::match_wildcard_for_single_variants,
    clippy::if_let_mutex,
    clippy::mismatched_target_os,
    clippy::await_holding_lock,
    clippy::match_on_vec_items,
    clippy::imprecise_flops,
    //clippy::suboptimal_flops,
    clippy::lossy_float_literal,
    clippy::rest_pat_in_fully_bound_structs,
    clippy::fn_params_excessive_bools,
    clippy::exit,
    clippy::inefficient_to_string,
    clippy::linkedlist,
    clippy::macro_use_imports,
    clippy::option_option,
    clippy::verbose_file_reads,
    rust_2018_idioms,
    future_incompatible,
    nonstandard_style
)]

mod ui;
pub use ui::*;

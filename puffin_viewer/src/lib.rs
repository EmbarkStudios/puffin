//! Remote puffin viewer, connecting to a [`puffin_http::Server`].

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
// END - Embark standard lints v0.5 for Rust 1.55+
// crate-specific exceptions:
#![allow(clippy::exit)]
#![cfg_attr(target_arch = "wasm32", allow(clippy::unused_unit))]

use eframe::egui;
use puffin::FrameView;
use puffin_egui::MaybeMutRef;

pub enum Source {
    None,
    Http(puffin_http::Client),
    FilePath(std::path::PathBuf, FrameView),
    FileName(String, FrameView),
}

impl Source {
    #[cfg(not(target_arch = "wasm32"))]
    fn frame_view(&self) -> FrameView {
        match self {
            Self::None => Default::default(),
            Self::Http(http_client) => http_client.frame_view().clone(),
            Self::FilePath(_, frame_view) | Self::FileName(_, frame_view) => frame_view.clone(),
        }
    }

    fn ui(&self, ui: &mut egui::Ui) {
        match self {
            Self::None => {
                ui.label("No file or stream open");
            }
            Self::Http(http_client) => {
                if http_client.connected() {
                    ui.label(format!("Connected to {}", http_client.addr()));
                } else {
                    ui.label(format!("Connecting to {}…", http_client.addr()));
                }
            }
            Self::FilePath(path, _) => {
                ui.label(format!("Viewing {}", path.display()));
            }
            Self::FileName(name, _) => {
                ui.label(format!("Viewing {}", name));
            }
        }
    }
}

pub struct PuffinViewer {
    profiler_ui: puffin_egui::ProfilerUi,
    source: Source,
    error: Option<String>,
    profile_self: bool,
    /// if [`Self::profile_self`] is checked, use this to introspect.
    global_profiler_ui: puffin_egui::GlobalProfilerUi,
}

impl PuffinViewer {
    pub fn new(source: Source) -> Self {
        Self {
            profiler_ui: Default::default(),
            source,
            error: None,
            profile_self: false,
            global_profiler_ui: Default::default(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn save_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("puffin", &["puffin"])
            .save_file()
        {
            if let Err(error) = self.source.frame_view().save_to_path(&path) {
                self.error = Some(format!("Failed to export: {}", error));
            } else {
                self.error = None;
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("puffin", &["puffin"])
            .pick_file()
        {
            self.open_puffin_path(path);
        }
    }

    fn open_puffin_path(&mut self, path: std::path::PathBuf) {
        puffin::profile_function!();
        match FrameView::load_path(&path) {
            Ok(frame_view) => {
                self.profiler_ui.reset();
                self.source = Source::FilePath(path, frame_view);
                self.error = None;
            }
            Err(err) => {
                self.error = Some(format!("Failed to load {}: {}", path.display(), err));
            }
        }
    }

    fn open_puffin_bytes(&mut self, name: String, bytes: &[u8]) {
        puffin::profile_function!();
        let mut reader = std::io::Cursor::new(bytes);
        match FrameView::load_reader(&mut reader) {
            Ok(frame_view) => {
                self.profiler_ui.reset();
                self.source = Source::FileName(name, frame_view);
                self.error = None;
            }
            Err(err) => {
                self.error = Some(format!("Failed to load file {:?}: {}", name, err));
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn ui_menu_bar(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if ctx.input().modifiers.command && ctx.input().key_pressed(egui::Key::O) {
            self.open_dialog();
        }

        if ctx.input().modifiers.command && ctx.input().key_pressed(egui::Key::S) {
            self.save_dialog();
        }

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                egui::widgets::global_dark_light_mode_switch(ui);
                ui.separator();

                ui.menu_button("File", |ui| {
                    if ui.button("Open…").clicked() {
                        self.open_dialog();
                    }

                    if ui.button("Save as…").clicked() {
                        self.save_dialog();
                    }

                    if ui.button("Quit").clicked() {
                        frame.close();
                    }
                });
                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.profile_self, "Profile self")
                        .on_hover_text("Show the flamegraph for puffin_viewer");
                });
            });
        });
    }

    fn ui_file_drag_and_drop(&mut self, ctx: &egui::Context) {
        use egui::*;

        // Preview hovering files:
        if !ctx.input().raw.hovered_files.is_empty() {
            let painter =
                ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("file_drop_target")));

            let screen_rect = ctx.input().screen_rect();
            painter.rect_filled(screen_rect, 0.0, Color32::from_black_alpha(192));
            painter.text(
                screen_rect.center(),
                Align2::CENTER_CENTER,
                "Drop to open .puffin file",
                TextStyle::Heading.resolve(&ctx.style()),
                Color32::WHITE,
            );
        }

        // Collect dropped files:
        if !ctx.input().raw.dropped_files.is_empty() {
            for file in &ctx.input().raw.dropped_files {
                if let Some(path) = &file.path {
                    self.open_puffin_path(path.clone());
                    break;
                } else if let Some(bytes) = &file.bytes {
                    self.open_puffin_bytes(file.name.clone(), bytes);
                    break;
                }
            }
        }
    }
}

impl eframe::App for PuffinViewer {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        puffin::GlobalProfiler::lock().new_frame();

        #[cfg(not(target_arch = "wasm32"))]
        {
            self.ui_menu_bar(ctx, _frame);
        }

        #[cfg(target_arch = "wasm32")]
        {
            egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
                ui.heading("Puffin Viewer, on the web");
                ui.horizontal_wrapped(|ui| {
                    ui.label("It is recommended that you instead use the native version: ");
                    ui.code("cargo install puffin_viewer");
                });
                ui.hyperlink("https://github.com/EmbarkStudios/puffin");
            });
        }

        egui::TopBottomPanel::bottom("info_bar").show(ctx, |ui| {
            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::RED, error);
                ui.separator();
            }

            if self.profile_self {
                ui.label("Profiling puffin_viewer");
            } else {
                self.source.ui(ui);
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.profile_self {
                self.global_profiler_ui.ui(ui);
            } else {
                match &mut self.source {
                    Source::None => {
                        ui.heading("Drag-and-drop a .puffin file here");
                    }
                    Source::Http(http_client) => {
                        self.profiler_ui
                            .ui(ui, &mut MaybeMutRef::MutRef(&mut http_client.frame_view()));
                    }
                    Source::FilePath(_, frame_view) | Source::FileName(_, frame_view) => {
                        self.profiler_ui.ui(ui, &mut MaybeMutRef::Ref(frame_view));
                    }
                }
            }
        });

        self.ui_file_drag_and_drop(ctx);
    }
}

// ----------------------------------------------------------------------------
// When compiling for web:

#[cfg(target_arch = "wasm32")]
use eframe::{
    wasm_bindgen::{self, prelude::*},
};

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct WebHandle {
    _handle: eframe::web::AppRunnerRef,
}

/// This is the entry-point for all the web-assembly.
/// This is called once from the HTML.
/// It loads the app, installs some callbacks, then returns.
/// You can add more callbacks like this if you want to call in to your code.
#[cfg(target_arch = "wasm32")]
#[allow(clippy::unused_unit)]
#[wasm_bindgen]
pub async fn start(canvas_id: &str) -> Result<WebHandle, eframe::wasm_bindgen::JsValue> {
    puffin::set_scopes_on(true); // quiet warning in `puffin_egui`.
    let web_options = eframe::WebOptions::default();
    eframe::start_web(
        canvas_id,
        web_options,
        Box::new(|_cc| Box::new(PuffinViewer::new(Source::None))),
    )
    .await
    .map(|handle| WebHandle { _handle: handle })
}

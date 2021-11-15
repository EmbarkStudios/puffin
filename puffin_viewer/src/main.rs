//! Remote puffin viewer, connecting to a [`puffin_http::PuffinServer`].

// BEGIN - Embark standard lints v0.4
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
    clippy::doc_markdown,
    clippy::empty_enum,
    clippy::enum_glob_use,
    clippy::exit,
    clippy::expl_impl_clone_on_copy,
    clippy::explicit_deref_methods,
    clippy::explicit_into_iter_loop,
    clippy::fallible_impl_from,
    clippy::filter_map_next,
    clippy::float_cmp_const,
    clippy::fn_params_excessive_bools,
    clippy::if_let_mutex,
    clippy::implicit_clone,
    clippy::imprecise_flops,
    clippy::inefficient_to_string,
    clippy::invalid_upcast_comparisons,
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
    clippy::match_wildcard_for_single_variants,
    clippy::mem_forget,
    clippy::mismatched_target_os,
    clippy::mut_mut,
    clippy::mutex_integer,
    clippy::needless_borrow,
    clippy::needless_continue,
    clippy::option_option,
    clippy::path_buf_push_overwrite,
    clippy::ptr_as_ptr,
    clippy::ref_option_ref,
    clippy::rest_pat_in_fully_bound_structs,
    clippy::same_functions_in_if_condition,
    clippy::semicolon_if_nothing_returned,
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
// END - Embark standard lints v0.4
// crate-specific exceptions:
#![allow(clippy::exit)]

use eframe::{egui, epi};
use puffin::FrameView;
use puffin_egui::MaybeMutRef;
use std::path::PathBuf;

/// puffin profile viewer.
///
/// Can either connect remotely to a puffin server
/// or open a .puffin recording file.
#[derive(argh::FromArgs)]
struct Arguments {
    /// which server to connect to, e.g. `127.0.0.1:8585`.
    #[argh(option, default = "default_url()")]
    url: String,

    /// what .puffin file to open, e.g. `my/recording.puffin`.
    #[argh(positional)]
    file: Option<String>,
}

fn default_url() -> String {
    format!("127.0.0.1:{}", puffin_http::DEFAULT_PORT)
}

fn main() {
    let opt: Arguments = argh::from_env();

    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()
        .ok();

    puffin::set_scopes_on(true); // quiet warning in `puffin_egui`.

    let app = if let Some(file) = opt.file {
        let path = PathBuf::from(file);
        match FrameView::load_path(&path) {
            Ok(frame_view) => PuffinViewer {
                profiler_ui: Default::default(),
                source: Source::FilePath(path, frame_view),
                error: None,
            },
            Err(err) => {
                log::error!("Failed to load {:?}: {}", path.display(), err);
                std::process::exit(1);
            }
        }
    } else {
        PuffinViewer {
            profiler_ui: Default::default(),
            source: Source::Http(puffin_http::Client::new(opt.url)),
            error: None,
        }
    };

    let options = epi::NativeOptions {
        drag_and_drop_support: true,
        ..Default::default()
    };
    eframe::run_native(Box::new(app), options);
}

pub enum Source {
    Http(puffin_http::Client),
    FilePath(PathBuf, FrameView),
    FileName(String, FrameView),
}

impl Source {
    fn frame_view(&self) -> FrameView {
        match self {
            Source::Http(http_client) => http_client.frame_view().clone(),
            Source::FilePath(_, frame_view) | Source::FileName(_, frame_view) => frame_view.clone(),
        }
    }

    fn ui(&self, ui: &mut egui::Ui) {
        match self {
            Source::Http(http_client) => {
                if http_client.connected() {
                    ui.label(format!("Connected to {}", http_client.addr()));
                } else {
                    ui.label(format!("Connecting to {}…", http_client.addr()));
                }
            }
            Source::FilePath(path, _) => {
                ui.label(format!("Viewing {}", path.display()));
            }
            Source::FileName(name, _) => {
                ui.label(format!("Viewing {}", name));
            }
        }
    }
}

pub struct PuffinViewer {
    profiler_ui: puffin_egui::ProfilerUi,
    source: Source,
    error: Option<String>,
}

impl PuffinViewer {
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

    fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("puffin", &["puffin"])
            .pick_file()
        {
            self.open_puffin_path(path);
        }
    }

    fn open_puffin_path(&mut self, path: std::path::PathBuf) {
        match FrameView::load_path(&path) {
            Ok(frame_view) => {
                self.profiler_ui.reset();
                self.source = Source::FilePath(path, frame_view);
                self.error = None;
            }
            Err(err) => {
                self.error = Some(format!("Failed to load {:?}: {}", path.display(), err));
            }
        }
    }

    fn open_puffin_bytes(&mut self, name: String, bytes: &[u8]) {
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

    fn ui_menu_bar(&mut self, ctx: &egui::CtxRef, frame: &mut epi::Frame<'_>) {
        if ctx.input().modifiers.command && ctx.input().key_pressed(egui::Key::O) {
            self.open_dialog();
        }

        if ctx.input().modifiers.command && ctx.input().key_pressed(egui::Key::S) {
            self.save_dialog();
        }

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                egui::menu::menu(ui, "File", |ui| {
                    if ui.button("Open…").clicked() {
                        self.open_dialog();
                    }

                    if ui.button("Save as…").clicked() {
                        self.save_dialog();
                    }

                    if ui.button("Quit").clicked() {
                        frame.quit();
                    }
                });
            });
        });
    }

    fn ui_file_drag_and_drop(&mut self, ctx: &egui::CtxRef) {
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
                TextStyle::Heading,
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

impl epi::App for PuffinViewer {
    fn name(&self) -> &str {
        "puffin http client viewer"
    }

    fn update(&mut self, ctx: &egui::CtxRef, frame: &mut epi::Frame<'_>) {
        self.ui_menu_bar(ctx, frame);

        egui::TopBottomPanel::bottom("info_bar").show(ctx, |ui| {
            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::RED, error);
                ui.separator();
            }

            self.source.ui(ui);
        });

        egui::CentralPanel::default().show(ctx, |ui| match &mut self.source {
            Source::Http(http_client) => {
                self.profiler_ui
                    .ui(ui, &mut MaybeMutRef::MutRef(&mut http_client.frame_view()));
            }
            Source::FilePath(_, frame_view) | Source::FileName(_, frame_view) => {
                self.profiler_ui.ui(ui, &mut MaybeMutRef::Ref(frame_view));
            }
        });

        self.ui_file_drag_and_drop(ctx);
    }
}

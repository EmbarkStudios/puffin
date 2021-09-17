//! Bindings for showing [`puffin`] profile scopes in [`egui`].
//!
//! Usage:
//! ```
//! # let mut egui_ctx = egui::CtxRef::default();
//! # egui_ctx.begin_frame(Default::default());
//! puffin_egui::profiler_window(&egui_ctx);
//! ```

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
#![allow(clippy::float_cmp, clippy::manual_range_contains)]

mod flamegraph;
mod stats;

pub use {egui, puffin};

use egui::*;
use puffin::*;
use std::sync::{Arc, Mutex};

const ERROR_COLOR: Color32 = Color32::RED;
const HOVER_COLOR: Rgba = Rgba::from_rgb(0.8, 0.8, 0.8);

// ----------------------------------------------------------------------------

/// Show an [`egui::Window`] with the profiler contents.
///
/// If you want to control the window yourself, use [`profiler_ui`] instead.
///
/// Returns `false` if the user closed the profile window.
pub fn profiler_window(ctx: &egui::CtxRef) -> bool {
    puffin::profile_function!();
    let mut open = true;
    egui::Window::new("Profiler")
        .default_size([1024.0, 600.0])
        .open(&mut open)
        .show(ctx, |ui| profiler_ui(ui));
    open
}

static PROFILE_UI: once_cell::sync::Lazy<Mutex<GlobalProfilerUi>> =
    once_cell::sync::Lazy::new(Default::default);

/// Show the profiler.
///
/// Call this from within an [`egui::Window`], or use [`profiler_window`] instead.
pub fn profiler_ui(ui: &mut egui::Ui) {
    let mut profile_ui = PROFILE_UI.lock().unwrap();
    profile_ui.ui(ui);
}

fn latest_frames(frame_view: &FrameView) -> Frames {
    Frames {
        recent: frame_view.recent_frames().cloned().collect(),
        slowest: frame_view.slowest_frames_chronological(),
    }
}

// ----------------------------------------------------------------------------

/// Show [`puffin::GlobalProfiler`], i.e. profile the app we are running in.
#[derive(Default)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct GlobalProfilerUi {
    #[cfg_attr(feature = "serde", serde(skip))]
    global_frame_view: GlobalFrameView,
    pub profiler_ui: ProfilerUi,
}

impl GlobalProfilerUi {
    /// Show an [`egui::Window`] with the profiler contents.
    ///
    /// If you want to control the window yourself, use [`Self::ui`] instead.
    ///
    /// Returns `false` if the user closed the profile window.
    pub fn window(&mut self, ctx: &egui::CtxRef) -> bool {
        let mut frame_view = self.global_frame_view.lock();
        self.profiler_ui.window(ctx, &mut frame_view)
    }

    /// Show the profiler.
    ///
    /// Call this from within an [`egui::Window`], or use [`Self::window`] instead.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let mut frame_view = self.global_frame_view.lock();
        self.profiler_ui.ui(ui, &mut frame_view);
    }
}

// ----------------------------------------------------------------------------

/// The frames we can select between
#[derive(Clone)]
pub struct Frames {
    pub recent: Vec<Arc<FrameData>>,
    pub slowest: Vec<Arc<FrameData>>,
}

#[derive(Clone)]
pub struct Paused {
    /// What we are viewing
    selected: Arc<FrameData>,
    frames: Frames,
}

#[derive(Copy, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum View {
    Flamegraph,
    Stats,
}

impl Default for View {
    fn default() -> Self {
        Self::Flamegraph
    }
}

/// Contains settings for the profiler.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct ProfilerUi {
    pub options: flamegraph::Options,

    pub view: View,

    /// If `None`, we show the latest frames.
    #[cfg_attr(feature = "serde", serde(skip))]
    paused: Option<Paused>,
}

impl ProfilerUi {
    pub fn reset(&mut self) {
        self.paused = None;
    }

    /// Show an [`egui::Window`] with the profiler contents.
    ///
    /// If you want to control the window yourself, use [`Self::ui`] instead.
    ///
    /// Returns `false` if the user closed the profile window.
    pub fn window(&mut self, ctx: &egui::CtxRef, frame_view: &mut FrameView) -> bool {
        puffin::profile_function!();
        let mut open = true;
        egui::Window::new("Profiler")
            .default_size([1024.0, 600.0])
            .open(&mut open)
            .show(ctx, |ui| self.ui(ui, frame_view));
        open
    }

    /// The frames we can select between
    fn frames(&self, frame_view: &FrameView) -> Frames {
        self.paused
            .as_ref()
            .map_or_else(|| latest_frames(frame_view), |paused| paused.frames.clone())
    }

    /// Pause on the specific frame
    fn pause_and_select(&mut self, frame_view: &FrameView, selected: Arc<FrameData>) {
        if let Some(paused) = &mut self.paused {
            paused.selected = selected;
        } else {
            self.paused = Some(Paused {
                selected,
                frames: self.frames(frame_view),
            });
        }
    }

    fn selected_frame(&self, frame_view: &FrameView) -> Option<Arc<FrameData>> {
        self.paused
            .as_ref()
            .map(|paused| paused.selected.clone())
            .or_else(|| frame_view.latest_frame())
    }

    fn selected_frame_index(&self, frame_view: &FrameView) -> Option<FrameIndex> {
        self.selected_frame(frame_view)
            .map(|frame| frame.frame_index)
    }

    /// Show the profiler.
    ///
    /// Call this from within an [`egui::Window`], or use [`Self::window`] instead.
    pub fn ui(&mut self, ui: &mut egui::Ui, frame_view: &mut FrameView) {
        #![allow(clippy::collapsible_else_if)]
        puffin::profile_function!();

        if !puffin::are_scopes_on() {
            ui.colored_label(ERROR_COLOR, "The puffin profiler is OFF!")
                .on_hover_text("Turn it on with puffin::set_scopes_on(true)");
        }

        let mut hovered_frame = None;

        egui::CollapsingHeader::new("Frames")
            .default_open(true)
            .show(ui, |ui| {
                hovered_frame = self.show_frames(ui, frame_view);
            });

        let frame = match hovered_frame.or_else(|| self.selected_frame(frame_view)) {
            Some(frame) => frame,
            None => {
                ui.label("No profiling data");
                return;
            }
        };

        // TODO: show age of data

        ui.horizontal(|ui| {
            let play_pause_button_size = Vec2::splat(24.0);
            if self.paused.is_some() {
                if ui
                    .add_sized(play_pause_button_size, egui::Button::new("▶"))
                    .on_hover_text("Show latest data. Toggle with space.")
                    .clicked()
                    || ui.input().key_pressed(egui::Key::Space)
                {
                    self.paused = None;
                }
            } else {
                ui.horizontal(|ui| {
                    if ui
                        .add_sized(play_pause_button_size, egui::Button::new("⏸"))
                        .on_hover_text("Pause on this frame. Toggle with space.")
                        .clicked()
                        || ui.input().key_pressed(egui::Key::Space)
                    {
                        let latest = frame_view.latest_frame();
                        if let Some(latest) = latest {
                            self.pause_and_select(frame_view, latest);
                        }
                    }
                });
            }
            ui.separator();
            let (min_ns, max_ns) = frame.range_ns;
            ui.label(format!(
                "Showing frame #{}, {:.1} ms, {} threads, {} scopes, {:.1} kB",
                frame.frame_index,
                (max_ns - min_ns) as f64 * 1e-6,
                frame.thread_streams.len(),
                frame.num_scopes,
                frame.num_bytes as f64 * 1e-3
            ));
        });

        if self.paused.is_none() {
            ui.ctx().request_repaint(); // keep refreshing to see latest data
        }

        ui.separator();

        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.view, View::Flamegraph, "Flamegraph");
            ui.selectable_value(&mut self.view, View::Stats, "Stats");
        });

        ui.separator();

        match self.view {
            View::Flamegraph => flamegraph::ui(ui, &mut self.options, &frame),
            View::Stats => stats::ui(ui, &frame),
        }
    }

    /// Returns hovered, if any
    fn show_frames(
        &mut self,
        ui: &mut egui::Ui,
        frame_view: &mut FrameView,
    ) -> Option<Arc<FrameData>> {
        let frames = self.frames(frame_view);

        let mut hovered_frame = None;

        let longest_count = frames.recent.len().max(frames.slowest.len());

        egui::Grid::new("frame_grid").num_columns(2).show(ui, |ui| {
            ui.label("Recent:");

            Frame::dark_canvas(ui.style()).show(ui, |ui| {
                self.show_frame_list(
                    ui,
                    frame_view,
                    &frames.recent,
                    longest_count,
                    &mut hovered_frame,
                );
            });

            ui.end_row();

            ui.vertical(|ui| {
                ui.style_mut().wrap = Some(false);
                ui.add_space(16.0); // make it a bit more centered
                ui.label("Slowest:");
                if ui.button("Clear").clicked() {
                    frame_view.clear_slowest();
                }
            });
            Frame::dark_canvas(ui.style()).show(ui, |ui| {
                self.show_frame_list(
                    ui,
                    frame_view,
                    &frames.slowest,
                    longest_count,
                    &mut hovered_frame,
                );
            });
        });

        hovered_frame
    }

    fn show_frame_list(
        &mut self,
        ui: &mut egui::Ui,
        frame_view: &FrameView,
        frames: &[Arc<FrameData>],
        longest_count: usize,
        hovered_frame: &mut Option<Arc<FrameData>>,
    ) {
        let mut slowest_frame = 0;
        for frame in frames {
            slowest_frame = frame.duration_ns().max(slowest_frame);
        }

        let desired_size = Vec2::new(
            ui.available_size_before_wrap_finite().x,
            self.options.frame_list_height,
        );
        let (response, painter) = ui.allocate_painter(desired_size, Sense::drag());
        let rect = response.rect;

        let frame_width_including_spacing =
            (rect.width() / (longest_count as f32)).max(4.0).min(20.0);
        let frame_spacing = 2.0;
        let frame_width = frame_width_including_spacing - frame_spacing;

        let selected_frame_index = self.selected_frame_index(frame_view);

        for (i, frame) in frames.iter().enumerate() {
            let x = rect.right() - (frames.len() as f32 - i as f32) * frame_width_including_spacing;
            let frame_rect = Rect::from_min_max(
                Pos2::new(x, rect.top()),
                Pos2::new(x + frame_width, rect.bottom()),
            );

            let duration = frame.duration_ns();

            let is_selected = Some(frame.frame_index) == selected_frame_index;

            let is_hovered = if let Some(mouse_pos) = response.hover_pos() {
                response.hovered()
                    && frame_rect
                        .expand2(vec2(0.5 * frame_spacing, 0.0))
                        .contains(mouse_pos)
            } else {
                false
            };

            if is_hovered {
                *hovered_frame = Some(frame.clone());
                egui::show_tooltip_at_pointer(ui.ctx(), Id::new("puffin_frame_tooltip"), |ui| {
                    ui.label(format!("{:.1} ms", frame.duration_ns() as f64 * 1e-6));
                });
            }
            if is_hovered && response.clicked() {
                self.pause_and_select(frame_view, frame.clone());
            }

            let color = if is_selected {
                Rgba::WHITE
            } else if is_hovered {
                HOVER_COLOR
            } else {
                Rgba::from_rgb(0.6, 0.6, 0.4)
            };

            // Transparent, full height:
            let alpha = if is_selected || is_hovered { 0.6 } else { 0.25 };
            painter.rect_filled(frame_rect, 0.0, color * alpha);

            // Opaque, height based on duration:
            let mut short_rect = frame_rect;
            short_rect.min.y = lerp(
                frame_rect.bottom_up_range(),
                duration as f32 / slowest_frame as f32,
            );
            painter.rect_filled(short_rect, 0.0, color);
        }
    }
}

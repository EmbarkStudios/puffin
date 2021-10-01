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
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

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

/// The frames we can chose between to select something to vis.
#[derive(Clone)]
pub struct AvailableFrames {
    pub recent: Vec<Arc<FrameData>>,
    pub slowest: Vec<Arc<FrameData>>,
}

impl AvailableFrames {
    fn latest(frame_view: &FrameView) -> Self {
        Self {
            recent: frame_view.recent_frames().cloned().collect(),
            slowest: frame_view.slowest_frames_chronological(),
        }
    }
}

/// Multiple streams for one thread.
#[derive(Clone)]
pub struct Streams {
    streams: Vec<Arc<StreamInfo>>,
    merges: Vec<MergeScope<'static>>,
    max_depth: usize,
}

impl Streams {
    pub fn from_vec(streams: Vec<Arc<StreamInfo>>) -> Self {
        crate::profile_function!();
        let merges = puffin::merge_scopes_in_streams(streams.iter().map(|si| &si.stream)).unwrap();
        let merges = merges.into_iter().map(|ms| ms.into_owned()).collect();

        let mut max_depth = 0;
        for stream_info in &streams {
            max_depth = stream_info.depth.max(max_depth);
        }

        Self {
            streams,
            merges,
            max_depth,
        }
    }
}

/// Selected frames ready to be viewed.
/// Never empty.
#[derive(Clone)]
pub struct SelectedFrames {
    /// ordered, but not necessarily in sequence
    pub frames: vec1::Vec1<Arc<FrameData>>,
    pub range_ns: (NanoSecond, NanoSecond),
    pub threads: HashMap<ThreadInfo, Streams>,
}

impl SelectedFrames {
    fn try_from_vec(frames: Vec<Arc<FrameData>>) -> Option<Self> {
        let frames = vec1::Vec1::try_from_vec(frames).ok()?;
        Some(Self::from_vec1(frames))
    }

    fn from_vec1(mut frames: vec1::Vec1<Arc<FrameData>>) -> Self {
        frames.sort_by_key(|f| f.frame_index);
        frames.dedup_by_key(|f| f.frame_index);

        let min_ns = frames.first().range_ns.0;
        let max_ns = frames.last().range_ns.1;

        let mut threads: HashMap<ThreadInfo, Vec<Arc<StreamInfo>>> = HashMap::new();
        for frame in &frames {
            for (ti, si) in &frame.thread_streams {
                threads.entry(ti.clone()).or_default().push(si.clone());
            }
        }

        let threads = threads
            .drain()
            .map(|(ti, streams)| (ti, Streams::from_vec(streams)))
            .collect();

        Self {
            frames,
            range_ns: (min_ns, max_ns),
            threads,
        }
    }

    /// Number of frames
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn contains(&self, frame_index: u64) -> bool {
        self.frames.iter().any(|f| f.frame_index == frame_index)
    }
}

#[derive(Clone)]
pub struct Paused {
    /// What we are viewing
    selected: SelectedFrames,
    frames: AvailableFrames,
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
    fn frames(&self, frame_view: &FrameView) -> AvailableFrames {
        self.paused.as_ref().map_or_else(
            || AvailableFrames::latest(frame_view),
            |paused| paused.frames.clone(),
        )
    }

    /// Pause on the specific frame
    fn pause_and_select(&mut self, frame_view: &FrameView, selected: SelectedFrames) {
        if let Some(paused) = &mut self.paused {
            paused.selected = selected;
        } else {
            self.paused = Some(Paused {
                selected,
                frames: self.frames(frame_view),
            });
        }
    }

    fn is_selected(&self, frame_view: &FrameView, frame_index: u64) -> bool {
        if let Some(paused) = &self.paused {
            paused.selected.contains(frame_index)
        } else if let Some(latest_frame) = frame_view.latest_frame() {
            latest_frame.frame_index == frame_index
        } else {
            false
        }
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

        let frames = if let Some(hovered_frame) = hovered_frame {
            SelectedFrames::try_from_vec(vec![hovered_frame])
        } else if let Some(paused) = &self.paused {
            Some(paused.selected.clone())
        } else if let Some(latest_frame) = frame_view.latest_frame() {
            SelectedFrames::try_from_vec(vec![latest_frame])
        } else {
            None
        };

        let frames = if let Some(frames) = frames {
            frames
        } else {
            ui.label("No profiling data");
            return;
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
                            self.pause_and_select(
                                frame_view,
                                SelectedFrames::from_vec1(vec1::vec1![latest]),
                            );
                        }
                    }
                });
            }
            ui.separator();
            frames_info_ui(ui, &frames);
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
            View::Flamegraph => flamegraph::ui(ui, &mut self.options, &frames),
            View::Stats => stats::ui(ui, &frames.frames),
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
        let (response, painter) = ui.allocate_painter(desired_size, Sense::click_and_drag());
        let rect = response.rect;

        let frame_width_including_spacing =
            (rect.width() / (longest_count as f32)).max(4.0).min(20.0);
        let frame_spacing = 2.0;
        let frame_width = frame_width_including_spacing - frame_spacing;

        let mut new_selection = vec![];

        for (i, frame) in frames.iter().enumerate() {
            let x = rect.right() - (frames.len() as f32 - i as f32) * frame_width_including_spacing;
            let frame_rect = Rect::from_min_max(
                Pos2::new(x, rect.top()),
                Pos2::new(x + frame_width, rect.bottom()),
            );

            let duration = frame.duration_ns();

            let is_selected = self.is_selected(frame_view, frame.frame_index);

            let is_hovered = if let Some(mouse_pos) = response.hover_pos() {
                response.hovered()
                    && !response.dragged()
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

            if response.dragged() {
                if let (Some(start), Some(curr)) = (
                    ui.input().pointer.press_origin(),
                    ui.input().pointer.interact_pos(),
                ) {
                    let min_x = start.x.min(curr.x);
                    let max_x = start.x.max(curr.x);
                    let intersects = min_x <= frame_rect.right() && frame_rect.left() <= max_x;
                    if intersects {
                        new_selection.push(frame.clone());
                    }
                }
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

        if let Some(new_selection) = SelectedFrames::try_from_vec(new_selection) {
            self.pause_and_select(frame_view, new_selection);
        }
    }
}

fn frames_info_ui(ui: &mut egui::Ui, frames: &SelectedFrames) {
    let frames = &frames.frames;

    let mut sum_ns = 0;
    let mut sum_scopes = 0;
    let mut sum_bytes = 0;

    let mut threads = std::collections::HashSet::new();

    for frame in frames {
        let (min_ns, max_ns) = frame.range_ns;
        sum_ns += max_ns - min_ns;

        threads.extend(frame.thread_streams.keys());
        sum_scopes += frame.num_scopes;
        sum_bytes += frame.num_bytes;
    }

    let frame_indices = if frames.len() == 1 {
        format!("frame #{}", frames[0].frame_index)
    } else if frames.len() as u64 == frames.last().frame_index - frames.first().frame_index + 1 {
        format!(
            "frames #{} - #{}",
            frames.first().frame_index,
            frames.last().frame_index
        )
    } else {
        format!("{} frames", frames.len())
    };

    ui.label(format!(
        "Showing {}, {:.1} ms, {} threads, {} scopes, {:.1} kB",
        frame_indices,
        sum_ns as f64 * 1e-6,
        threads.len(),
        sum_scopes,
        sum_bytes as f64 * 1e-3
    ));
}

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
    collections::{BTreeMap, BTreeSet},
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

    /// The frames we are looking at.
    pub fn global_frame_view(&self) -> &GlobalFrameView {
        &self.global_frame_view
    }
}

// ----------------------------------------------------------------------------

/// The frames we can chose between when selecting what frame(s) to view.
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

    fn all_uniq(&self) -> Vec<Arc<FrameData>> {
        let mut all = self.slowest.clone();
        all.extend(self.recent.iter().cloned());
        all.sort_by_key(|frame| frame.frame_index());
        all.dedup_by_key(|frame| frame.frame_index());
        all
    }
}

/// Multiple streams for one thread.
#[derive(Clone)]
pub struct Streams {
    streams: Vec<Arc<StreamInfo>>,
    merged_scopes: Vec<MergeScope<'static>>,
    max_depth: usize,
}

impl Streams {
    fn new(frames: &[Arc<UnpackedFrameData>], thread_info: &ThreadInfo) -> Self {
        crate::profile_function!();

        let mut streams = vec![];
        for frame in frames {
            if let Some(stream_info) = frame.thread_streams.get(thread_info) {
                streams.push(stream_info.clone());
            }
        }

        let merges = puffin::merge_scopes_for_thread(frames, thread_info).unwrap();
        let merges = merges.into_iter().map(|ms| ms.into_owned()).collect();

        let mut max_depth = 0;
        for stream_info in &streams {
            max_depth = stream_info.depth.max(max_depth);
        }

        Self {
            streams,
            merged_scopes: merges,
            max_depth,
        }
    }
}

/// Selected frames ready to be viewed.
/// Never empty.
#[derive(Clone)]
pub struct SelectedFrames {
    /// ordered, but not necessarily in sequence
    pub frames: vec1::Vec1<Arc<UnpackedFrameData>>,
    pub raw_range_ns: (NanoSecond, NanoSecond),
    pub merged_range_ns: (NanoSecond, NanoSecond),
    pub threads: BTreeMap<ThreadInfo, Streams>,
}

impl SelectedFrames {
    fn try_from_vec(frames: Vec<Arc<UnpackedFrameData>>) -> Option<Self> {
        let frames = vec1::Vec1::try_from_vec(frames).ok()?;
        Some(Self::from_vec1(frames))
    }

    fn from_vec1(mut frames: vec1::Vec1<Arc<UnpackedFrameData>>) -> Self {
        puffin::profile_function!();
        frames.sort_by_key(|f| f.frame_index());
        frames.dedup_by_key(|f| f.frame_index());

        let mut threads: BTreeSet<ThreadInfo> = BTreeSet::new();
        for frame in &frames {
            for ti in frame.thread_streams.keys() {
                threads.insert(ti.clone());
            }
        }

        let threads: BTreeMap<ThreadInfo, Streams> = threads
            .iter()
            .map(|ti| (ti.clone(), Streams::new(&frames, ti)))
            .collect();

        let mut merged_min_ns = NanoSecond::MAX;
        let mut merged_max_ns = NanoSecond::MIN;
        for stream in threads.values() {
            for scope in &stream.merged_scopes {
                let scope_start = scope.relative_start_ns;
                let scope_end = scope_start + scope.duration_per_frame_ns;
                merged_min_ns = merged_min_ns.min(scope_start);
                merged_max_ns = merged_max_ns.max(scope_end);
            }
        }

        let raw_range_ns = (
            frames.first().meta.range_ns.0,
            frames.last().meta.range_ns.1,
        );

        Self {
            frames,
            raw_range_ns,
            merged_range_ns: (merged_min_ns, merged_max_ns),
            threads,
        }
    }

    pub fn contains(&self, frame_index: u64) -> bool {
        self.frames.iter().any(|f| f.frame_index() == frame_index)
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
#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct ProfilerUi {
    pub options: flamegraph::Options,

    pub view: View,

    /// If `None`, we show the latest frames.
    #[cfg_attr(feature = "serde", serde(skip))]
    paused: Option<Paused>,

    /// Used to normalize frame height in frame view
    slowest_frame: f32,
}

impl Default for ProfilerUi {
    fn default() -> Self {
        Self {
            options: Default::default(),
            view: Default::default(),
            paused: None,
            slowest_frame: 0.16,
        }
    }
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
            latest_frame.frame_index() == frame_index
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

        if frame_view.is_empty() {
            ui.label("No profiling data");
            return;
        };

        let mut hovered_frame = None;

        egui::CollapsingHeader::new("Frames")
            .default_open(true)
            .show(ui, |ui| {
                hovered_frame = self.show_frames(ui, frame_view);
            });

        let frames = if let Some(frame) = hovered_frame {
            match frame.unpack() {
                Ok(frame) => SelectedFrames::try_from_vec(vec![frame]),
                Err(err) => {
                    ui.colored_label(
                        ERROR_COLOR,
                        format!("Failed to load hovered frame: {}", err),
                    );
                    return;
                }
            }
        } else if let Some(paused) = &self.paused {
            Some(paused.selected.clone())
        } else if let Some(frame) = frame_view.latest_frame() {
            match frame.unpack() {
                Ok(frame) => SelectedFrames::try_from_vec(vec![frame]),
                Err(err) => {
                    ui.colored_label(ERROR_COLOR, format!("Failed to load latest frame: {}", err));
                    return;
                }
            }
        } else {
            None
        };

        let frames = if let Some(frames) = frames {
            frames
        } else {
            ui.label("No profiling data");
            return;
        };

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
                            if let Ok(latest) = latest.unpack() {
                                self.pause_and_select(
                                    frame_view,
                                    SelectedFrames::from_vec1(vec1::vec1![latest]),
                                );
                            }
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
        puffin::profile_function!();

        let frames = self.frames(frame_view);

        let mut hovered_frame = None;

        egui::Grid::new("frame_grid").num_columns(2).show(ui, |ui| {
            ui.label("");
            ui.label("Click to select a frame, or drag to select multiple frames.");
            ui.end_row();

            ui.label("Recent:");

            Frame::dark_canvas(ui.style()).show(ui, |ui| {
                egui::ScrollArea::horizontal()
                    .stick_to_right()
                    .show(ui, |ui| {
                        let slowest_visible = self.show_frame_list(
                            ui,
                            frame_view,
                            &frames.recent,
                            false,
                            &mut hovered_frame,
                            self.slowest_frame,
                        );
                        // quickly, but smoothly, normalize frame height:
                        self.slowest_frame = lerp(self.slowest_frame..=slowest_visible as f32, 0.2);
                    });
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

            // Show as many slow frames as we fit in the view:
            Frame::dark_canvas(ui.style()).show(ui, |ui| {
                let num_fit =
                    (ui.available_size_before_wrap().x / self.options.frame_width).floor();
                let num_fit = (num_fit as usize).at_least(1).at_most(frames.slowest.len());
                let slowest_of_the_slow = puffin::select_slowest(&frames.slowest, num_fit);

                let mut slowest_frame = 0;
                for frame in &slowest_of_the_slow {
                    slowest_frame = frame.duration_ns().max(slowest_frame);
                }

                self.show_frame_list(
                    ui,
                    frame_view,
                    &slowest_of_the_slow,
                    true,
                    &mut hovered_frame,
                    slowest_frame as f32,
                );
            });
        });

        {
            let uniq = frames.all_uniq();
            let mut bytes = 0;
            let mut unpacked = 0;
            for frame in &uniq {
                bytes += frame.bytes_of_ram_used();
                unpacked += frame.has_unpacked() as usize;
            }
            ui.label(format!(
                "{} frames ({} unpacked) using approximately {:.1} MB.",
                uniq.len(),
                unpacked,
                bytes as f64 * 1e-6
            ));
        }

        hovered_frame
    }

    /// Returns the slowest visible frame
    fn show_frame_list(
        &mut self,
        ui: &mut egui::Ui,
        frame_view: &FrameView,
        frames: &[Arc<FrameData>],
        tight: bool,
        hovered_frame: &mut Option<Arc<FrameData>>,
        slowest_frame: f32,
    ) -> NanoSecond {
        let frame_width_including_spacing = self.options.frame_width;

        let desired_width = if tight {
            frames.len() as f32 * frame_width_including_spacing
        } else {
            // leave gaps in the view for the missing frames
            let num_frames = frames[frames.len() - 1].frame_index() + 1 - frames[0].frame_index();
            num_frames as f32 * frame_width_including_spacing
        };

        let desired_size = Vec2::new(desired_width, self.options.frame_list_height);
        let (response, painter) = ui.allocate_painter(desired_size, Sense::click_and_drag());
        let rect = response.rect;

        let frame_spacing = 2.0;
        let frame_width = frame_width_including_spacing - frame_spacing;

        let viewing_multiple_frames = if let Some(paused) = &self.paused {
            paused.selected.frames.len() > 1 && !self.options.merge_scopes
        } else {
            false
        };

        let mut new_selection = vec![];
        let mut slowest_visible_frame = 0;

        for (i, frame) in frames.iter().enumerate() {
            let x = if tight {
                rect.right() - (frames.len() as f32 - i as f32) * frame_width_including_spacing
            } else {
                let latest_frame_index = frames[frames.len() - 1].frame_index();
                rect.right()
                    - (latest_frame_index + 1 - frame.frame_index()) as f32
                        * frame_width_including_spacing
            };

            let frame_rect = Rect::from_min_max(
                Pos2::new(x, rect.top()),
                Pos2::new(x + frame_width, rect.bottom()),
            );

            if ui.clip_rect().intersects(frame_rect) {
                let duration = frame.duration_ns();
                slowest_visible_frame = duration.max(slowest_visible_frame);

                let is_selected = self.is_selected(frame_view, frame.frame_index());

                let is_hovered = if let Some(mouse_pos) = response.hover_pos() {
                    response.hovered()
                        && !response.dragged()
                        && frame_rect
                            .expand2(vec2(0.5 * frame_spacing, 0.0))
                            .contains(mouse_pos)
                } else {
                    false
                };

                // preview when hovering is really annoying when viewing multiple frames
                if is_hovered && !is_selected && !viewing_multiple_frames {
                    *hovered_frame = Some(frame.clone());
                    egui::show_tooltip_at_pointer(
                        ui.ctx(),
                        Id::new("puffin_frame_tooltip"),
                        |ui| {
                            ui.label(format!("{:.1} ms", frame.duration_ns() as f64 * 1e-6));
                        },
                    );
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
                            if let Ok(frame) = frame.unpack() {
                                new_selection.push(frame);
                            }
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
        }

        if let Some(new_selection) = SelectedFrames::try_from_vec(new_selection) {
            self.pause_and_select(frame_view, new_selection);
        }

        slowest_visible_frame
    }
}

fn frames_info_ui(ui: &mut egui::Ui, selection: &SelectedFrames) {
    let mut sum_ns = 0;
    let mut sum_scopes = 0;

    for frame in &selection.frames {
        let (min_ns, max_ns) = frame.range_ns();
        sum_ns += max_ns - min_ns;

        sum_scopes += frame.meta.num_scopes;
    }

    let frame_indices = if selection.frames.len() == 1 {
        format!("frame #{}", selection.frames[0].frame_index())
    } else if selection.frames.len() as u64
        == selection.frames.last().frame_index() - selection.frames.first().frame_index() + 1
    {
        format!(
            "{} frames (#{} - #{})",
            selection.frames.len(),
            selection.frames.first().frame_index(),
            selection.frames.last().frame_index()
        )
    } else {
        format!("{} frames", selection.frames.len())
    };

    let mut info = format!(
        "Showing {}, {:.1} ms, {} threads, {} scopes.",
        frame_indices,
        sum_ns as f64 * 1e-6,
        selection.threads.len(),
        sum_scopes,
    );
    if let Some(time) = format_time(selection.raw_range_ns.0) {
        info += &format!(" Recorded {}.", time);
    }

    ui.label(info);
}

fn format_time(nanos: NanoSecond) -> Option<String> {
    let years_since_epoch = nanos / 1_000_000_000 / 60 / 60 / 24 / 365;
    if 50 <= years_since_epoch && years_since_epoch <= 150 {
        use chrono::TimeZone as _;
        let datetime = chrono::Utc.timestamp(nanos / 1_000_000_000, (nanos % 1_000_000_000) as _);
        Some(datetime.format("%Y-%m-%d %H:%M:%S%.3f UTC").to_string())
    } else {
        None // `nanos` is likely not counting from epoch.
    }
}

//! Bindings for showing [`puffin`] profile scopes in [`egui`].
//!
//! Usage:
//! ```
//! # let mut egui_ctx = egui::CtxRef::default();
//! # egui_ctx.begin_frame(Default::default());
//! puffin_egui::profiler_window(&egui_ctx);
//! ```

#![cfg_attr(not(debug_assertions), deny(warnings))] // Forbid warnings in release builds
#![deny(broken_intra_doc_links)]
#![deny(invalid_codeblock_attributes)]
#![deny(private_intra_doc_links)]
#![forbid(unsafe_code)]
#![warn(
    clippy::all,
    clippy::await_holding_lock,
    clippy::dbg_macro,
    clippy::debug_assert_with_mut_call,
    clippy::doc_markdown,
    clippy::empty_enum,
    clippy::enum_glob_use,
    clippy::exit,
    clippy::explicit_into_iter_loop,
    clippy::filter_map_next,
    clippy::fn_params_excessive_bools,
    clippy::if_let_mutex,
    clippy::imprecise_flops,
    clippy::inefficient_to_string,
    clippy::large_types_passed_by_value,
    clippy::let_unit_value,
    clippy::linkedlist,
    clippy::lossy_float_literal,
    clippy::macro_use_imports,
    clippy::map_err_ignore,
    clippy::map_flatten,
    clippy::match_on_vec_items,
    clippy::match_same_arms,
    clippy::match_wildcard_for_single_variants,
    clippy::mem_forget,
    clippy::mismatched_target_os,
    clippy::missing_errors_doc,
    clippy::missing_safety_doc,
    clippy::needless_borrow,
    clippy::needless_continue,
    clippy::needless_pass_by_value,
    clippy::option_option,
    clippy::pub_enum_variant_names,
    clippy::ref_option_ref,
    clippy::rest_pat_in_fully_bound_structs,
    clippy::string_add_assign,
    clippy::string_add,
    clippy::string_to_string,
    clippy::todo,
    clippy::unimplemented,
    clippy::unnested_or_patterns,
    clippy::unused_self,
    clippy::verbose_file_reads,
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms
)]
#![allow(clippy::float_cmp)]
#![allow(clippy::manual_range_contains)]

pub use {egui, puffin};

use egui::*;
use puffin::*;
use std::sync::Arc;
use std::sync::Mutex;

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

static PROFILE_UI: once_cell::sync::Lazy<Mutex<ProfilerUi>> =
    once_cell::sync::Lazy::new(Default::default);

/// Show the profiler.
///
/// Call this from within an [`egui::Window`], or use [`profiler_window`] instead.
pub fn profiler_ui(ui: &mut egui::Ui) {
    let mut profile_ui = PROFILE_UI.lock().unwrap();
    profile_ui.ui(ui)
}

// ----------------------------------------------------------------------------

const ERROR_COLOR: Color32 = Color32::RED;
const HOVER_COLOR: Rgba = Rgba::from_rgb(0.8, 0.8, 0.8);
const TEXT_STYLE: TextStyle = TextStyle::Body;

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

/// Contains settings for the profiler.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "persistence", serde(default))]
pub struct ProfilerUi {
    options: Options,

    /// If `None`, we show the latest frames.
    #[cfg_attr(feature = "serde", serde(skip))]
    paused: Option<Paused>,
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
struct Options {
    // --------------------
    // View:
    /// Controls zoom
    canvas_width_ns: f32,

    /// How much we have panned sideways:
    sideways_pan_in_points: f32,

    // --------------------
    // Interact:
    scroll_zoom_speed: f32,

    // --------------------
    // Visuals:
    /// Events shorter than this many points aren't painted
    cull_width: f32,
    rect_height: f32,
    spacing: f32,
    rounding: f32,

    /// Aggregate child scopes with the same id?
    merge_scopes: bool,

    /// Set when user clicks a scope.
    /// First part is `now()`, second is range.
    #[cfg_attr(feature = "serde", serde(skip))]
    zoom_to_relative_ns_range: Option<(f64, (NanoSecond, NanoSecond))>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            canvas_width_ns: 0.0,
            sideways_pan_in_points: 0.0,

            scroll_zoom_speed: 0.01,

            cull_width: 0.5,
            rect_height: 16.0,
            spacing: 4.0,
            rounding: 4.0,

            merge_scopes: true,

            zoom_to_relative_ns_range: None,
        }
    }
}

/// Context for painting a frame.
struct Info {
    ctx: egui::CtxRef,
    /// Bounding box of canvas in points:
    canvas: Rect,
    /// Interaction with the profiler canvas
    response: Response,
    painter: egui::Painter,
    text_height: f32,
    /// Time of first event
    start_ns: NanoSecond,
    /// Time of last event
    stop_ns: NanoSecond,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PaintResult {
    Culled,
    Hovered,
    Normal,
}

impl Info {
    fn point_from_ns(&self, options: &Options, ns: NanoSecond) -> f32 {
        self.canvas.min.x
            + options.sideways_pan_in_points
            + self.canvas.width() * ((ns - self.start_ns) as f32) / options.canvas_width_ns
    }
}

fn latest_frames() -> Frames {
    let profiler = GlobalProfiler::lock();
    Frames {
        recent: profiler.recent_frames().cloned().collect(),
        slowest: profiler.slowest_frames_chronological(),
    }
}

impl ProfilerUi {
    /// Show an [`egui::Window`] with the profiler contents.
    ///
    /// If you want to control the window yourself, use [`Self::ui`] instead.
    ///
    /// Returns `false` if the user closed the profile window.
    pub fn window(&mut self, ctx: &egui::CtxRef) -> bool {
        puffin::profile_function!();
        let mut open = true;
        egui::Window::new("Profiler")
            .default_size([1024.0, 600.0])
            .open(&mut open)
            .show(ctx, |ui| self.ui(ui));
        open
    }

    /// The frames we can select between
    fn frames(&self) -> Frames {
        self.paused
            .as_ref()
            .map_or_else(latest_frames, |paused| paused.frames.clone())
    }

    /// Pause on the specific frame
    fn pause_and_select(&mut self, selected: Arc<FrameData>) {
        if let Some(paused) = &mut self.paused {
            paused.selected = selected;
        } else {
            self.paused = Some(Paused {
                selected,
                frames: self.frames(),
            });
        }
    }

    fn selected_frame(&self) -> Option<Arc<FrameData>> {
        self.paused
            .as_ref()
            .map(|paused| paused.selected.clone())
            .or_else(|| GlobalProfiler::lock().latest_frame())
    }

    fn selected_frame_index(&self) -> Option<FrameIndex> {
        self.selected_frame().map(|frame| frame.frame_index)
    }

    /// Show the profiler.
    ///
    /// Call this from within an [`egui::Window`], or use [`Self::window`] instead.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
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
                hovered_frame = self.show_frames(ui);

                if self.paused.is_some() {
                    if ui.button("Resume / Show latest").clicked() {
                        self.paused = None;
                    }
                } else {
                    ui.horizontal(|ui| {
                        if ui.button("Pause").clicked() {
                            let latest = GlobalProfiler::lock().latest_frame();
                            if let Some(latest) = latest {
                                self.pause_and_select(latest);
                            }
                        }
                        if ui.button("Forget slowest frames").clicked() {
                            GlobalProfiler::lock().clear_slowest();
                        }
                    });
                }
            });

        let frame = match hovered_frame.or_else(|| self.selected_frame()) {
            Some(frame) => frame,
            None => {
                ui.label("No profiling data");
                return;
            }
        };

        // TODO: show age of data

        let (min_ns, max_ns) = frame.range_ns;

        ui.label(format!(
            "Current frame: {:.1} ms, {} threads, {} scopes, {:.1} kB",
            (max_ns - min_ns) as f64 * 1e-6,
            frame.thread_streams.len(),
            frame.num_scopes,
            frame.num_bytes as f64 * 1e-3
        ));

        ui.horizontal(|ui| {
            ui.label("Drag to pan. Scroll to zoom. Click to focus. Double-click to reset.");

            ui.checkbox(
                &mut self.options.merge_scopes,
                "Merge children with same ID",
            );
        });

        Frame::dark_canvas(ui.style()).show(ui, |ui| {
            self.ui_canvas(ui, &frame, (min_ns, max_ns));
        });
    }

    fn ui_canvas(
        &mut self,
        ui: &mut egui::Ui,
        frame: &FrameData,
        (min_ns, max_ns): (NanoSecond, NanoSecond),
    ) {
        puffin::profile_function!();

        let (response, painter) =
            ui.allocate_painter(ui.available_size_before_wrap_finite(), Sense::drag());

        let mut info = Info {
            ctx: ui.ctx().clone(),
            canvas: response.rect,
            response,
            painter,
            text_height: 15.0, // TODO
            start_ns: min_ns,
            stop_ns: max_ns,
        };
        self.interact_with_canvas(&info);

        if self.options.canvas_width_ns <= 0.0 {
            self.options.canvas_width_ns = (max_ns - min_ns) as f32;
            self.options.zoom_to_relative_ns_range = None;
        }

        paint_timeline(&info, &self.options, min_ns, max_ns);

        // We paint the threads bottom up
        let mut cursor_y = info.canvas.max.y;
        cursor_y -= info.text_height; // Leave room for time labels

        for (thread, stream) in &frame.thread_streams {
            // Visual separator between threads:
            info.painter.line_segment(
                [
                    pos2(info.canvas.min.x, cursor_y),
                    pos2(info.canvas.max.x, cursor_y),
                ],
                Stroke::new(1.0, Rgba::from_white_alpha(0.5)),
            );

            cursor_y -= info.text_height;
            let text_pos = pos2(info.canvas.min.x, cursor_y);
            paint_thread_info(&info, thread, text_pos);
            info.canvas.max.y = cursor_y;

            let mut paint_stream = || -> Result<()> {
                let top_scopes = Reader::from_start(stream).read_top_scopes()?;
                if self.options.merge_scopes {
                    let merges = puffin::merge_top_scopes(&top_scopes);
                    for merge in merges {
                        paint_merge_scope(
                            &info,
                            &mut self.options,
                            stream,
                            &merge,
                            0,
                            &mut cursor_y,
                        )?;
                    }
                } else {
                    for scope in top_scopes {
                        paint_scope(&info, &mut self.options, stream, &scope, 0, &mut cursor_y)?;
                    }
                }
                Ok(())
            };
            if let Err(err) = paint_stream() {
                let text = format!("Profiler stream error: {:?}", err);
                info.painter.text(
                    pos2(info.canvas.min.x, cursor_y),
                    Align2::LEFT_TOP,
                    text,
                    TEXT_STYLE,
                    ERROR_COLOR,
                );
            }

            cursor_y -= info.text_height; // Extra spacing between threads
        }
    }

    /// Returns hovered, if any
    fn show_frames(&mut self, ui: &mut egui::Ui) -> Option<Arc<FrameData>> {
        let frames = self.frames();

        let mut hovered_frame = None;

        let longest_count = frames.recent.len().max(frames.slowest.len());

        ui.label("Recent history:");
        Frame::dark_canvas(ui.style()).show(ui, |ui| {
            self.show_frame_list(ui, &frames.recent, longest_count, &mut hovered_frame);
        });
        ui.label("Slowest frames:");
        Frame::dark_canvas(ui.style()).show(ui, |ui| {
            self.show_frame_list(ui, &frames.slowest, longest_count, &mut hovered_frame);
        });

        hovered_frame
    }

    fn show_frame_list(
        &mut self,
        ui: &mut egui::Ui,
        frames: &[Arc<FrameData>],
        longest_count: usize,
        hovered_frame: &mut Option<Arc<FrameData>>,
    ) {
        let mut slowest_frame = 0;
        for frame in frames {
            slowest_frame = frame.duration_ns().max(slowest_frame);
        }

        let desired_size = Vec2::new(ui.available_size_before_wrap_finite().x, 48.0);
        let (response, painter) = ui.allocate_painter(desired_size, Sense::drag());
        let rect = response.rect;

        let frame_width_including_spacing =
            (rect.width() / (longest_count as f32)).max(4.0).min(20.0);
        let frame_spacing = 2.0;
        let frame_width = frame_width_including_spacing - frame_spacing;

        let selected_frame_index = self.selected_frame_index();

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
                self.pause_and_select(frame.clone());
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

    fn interact_with_canvas(&mut self, info: &Info) {
        if info.response.drag_delta().x != 0.0 {
            self.options.sideways_pan_in_points += info.response.drag_delta().x;
            self.options.zoom_to_relative_ns_range = None;
        }

        if info.response.hovered() {
            // Sideways pan with e.g. a touch pad:
            if info.ctx.input().scroll_delta.x != 0.0 {
                self.options.sideways_pan_in_points += info.ctx.input().scroll_delta.x;
                self.options.zoom_to_relative_ns_range = None;
            }

            let scroll_zoom =
                (info.ctx.input().scroll_delta.y * self.options.scroll_zoom_speed).exp();
            let zoom_factor = scroll_zoom * info.ctx.input().zoom_delta_2d().x;

            if zoom_factor != 1.0 {
                self.options.canvas_width_ns /= zoom_factor;

                if let Some(mouse_pos) = info.response.hover_pos() {
                    let zoom_center = mouse_pos.x - info.canvas.min.x;
                    self.options.sideways_pan_in_points =
                        (self.options.sideways_pan_in_points - zoom_center) * zoom_factor
                            + zoom_center;
                }
                self.options.zoom_to_relative_ns_range = None;
            }
        }

        if info.response.double_clicked() {
            // Reset view
            self.options.zoom_to_relative_ns_range =
                Some((info.ctx.input().time, (0, info.stop_ns - info.start_ns)));
        }

        if let Some((start_time, (start_ns, end_ns))) = self.options.zoom_to_relative_ns_range {
            const ZOOM_DURATION: f32 = 0.75;
            let t = ((info.ctx.input().time - start_time) as f32 / ZOOM_DURATION).min(1.0);

            let canvas_width = info.response.rect.width();

            let target_canvas_width_ns = (end_ns - start_ns) as f32;
            let target_pan_in_points = -canvas_width * start_ns as f32 / target_canvas_width_ns;

            // self.options.canvas_width_ns =
            //     lerp(self.options.canvas_width_ns..=target_canvas_width_ns, t);
            self.options.canvas_width_ns = lerp(
                self.options.canvas_width_ns.recip()..=target_canvas_width_ns.recip(),
                t,
            )
            .recip();
            self.options.sideways_pan_in_points = lerp(
                self.options.sideways_pan_in_points..=target_pan_in_points,
                t,
            );

            if t >= 1.0 {
                self.options.zoom_to_relative_ns_range = None;
            }

            info.ctx.request_repaint();
        }
    }
}

fn paint_timeline(info: &Info, options: &Options, start_ns: NanoSecond, stop_ns: NanoSecond) {
    if options.canvas_width_ns <= 0.0 {
        return;
    }

    // We show all measurements relative to start_ns

    let max_lines = info.canvas.width() / 4.0;
    let mut grid_spacing_ns = 1_000;
    while options.canvas_width_ns / (grid_spacing_ns as f32) > max_lines {
        grid_spacing_ns *= 10;
    }

    // We fade in lines as we zoom in:
    let num_tiny_lines = options.canvas_width_ns / (grid_spacing_ns as f32);
    let zoom_factor = remap_clamp(num_tiny_lines, (0.1 * max_lines)..=max_lines, 1.0..=0.0);
    let zoom_factor = zoom_factor * zoom_factor;
    let big_alpha = remap_clamp(zoom_factor, 0.0..=1.0, 0.5..=1.0);
    let medium_alpha = remap_clamp(zoom_factor, 0.0..=1.0, 0.1..=0.5);
    let tiny_alpha = remap_clamp(zoom_factor, 0.0..=1.0, 0.0..=0.1);

    let mut grid_ns = 0;

    loop {
        if start_ns + grid_ns > stop_ns {
            break; // stop grid where data stops
        }
        let line_x = info.point_from_ns(options, start_ns + grid_ns);
        if line_x > info.canvas.max.x {
            break;
        }

        if info.canvas.min.x <= line_x {
            let big_line = grid_ns % (grid_spacing_ns * 100) == 0;
            let medium_line = grid_ns % (grid_spacing_ns * 10) == 0;

            let line_alpha = if big_line {
                big_alpha
            } else if medium_line {
                medium_alpha
            } else {
                tiny_alpha
            };

            info.painter.line_segment(
                [
                    pos2(line_x, info.canvas.min.y),
                    pos2(line_x, info.canvas.max.y),
                ],
                Stroke::new(1.0, Rgba::from_white_alpha(line_alpha)),
            );

            let text_alpha = if big_line {
                medium_alpha
            } else if medium_line {
                tiny_alpha
            } else {
                0.0
            };

            if text_alpha > 0.0 {
                let text = grid_text(grid_ns);
                let text_x = line_x + 4.0;
                let text_color = Rgba::from_white_alpha((text_alpha * 2.0).min(1.0)).into();

                // Text at top:
                info.painter.text(
                    pos2(text_x, info.canvas.min.y),
                    Align2::LEFT_TOP,
                    &text,
                    TEXT_STYLE,
                    text_color,
                );

                // Text at bottom:
                info.painter.text(
                    pos2(text_x, info.canvas.max.y - info.text_height),
                    Align2::LEFT_TOP,
                    &text,
                    TEXT_STYLE,
                    text_color,
                );
            }
        }

        grid_ns += grid_spacing_ns;
    }
}

fn grid_text(grid_ns: NanoSecond) -> String {
    let grid_ms = to_ms(grid_ns);
    if grid_ns % 1_000_000 == 0 {
        format!("{:.0} ms", grid_ms)
    } else if grid_ns % 100_000 == 0 {
        format!("{:.1} ms", grid_ms)
    } else if grid_ns % 10_000 == 0 {
        format!("{:.2} ms", grid_ms)
    } else {
        format!("{:.3} ms", grid_ms)
    }
}

fn paint_record(
    info: &Info,
    options: &mut Options,
    prefix: &str,
    record: &Record<'_>,
    top_y: f32,
) -> PaintResult {
    let start_x = info.point_from_ns(options, record.start_ns);
    let stop_x = info.point_from_ns(options, record.stop_ns());
    let width = stop_x - start_x;
    if info.canvas.max.x < start_x || stop_x < info.canvas.min.x || width < options.cull_width {
        return PaintResult::Culled;
    }

    let bottom_y = top_y + options.rect_height;

    let is_hovered = if let Some(mouse_pos) = info.response.hover_pos() {
        start_x <= mouse_pos.x
            && mouse_pos.x <= stop_x
            && top_y <= mouse_pos.y
            && mouse_pos.y <= bottom_y
    } else {
        false
    };

    if is_hovered && info.response.clicked() {
        options.zoom_to_relative_ns_range = Some((
            info.ctx.input().time,
            (
                record.start_ns - info.start_ns,
                record.stop_ns() - info.start_ns,
            ),
        ));
    }

    let rect_min = pos2(start_x, top_y);
    let rect_max = pos2(stop_x, bottom_y);
    let rect_color = if is_hovered {
        HOVER_COLOR
    } else {
        // options.rect_color
        color_from_duration(record.duration_ns)
    };

    info.painter.rect_filled(
        Rect::from_min_max(rect_min, rect_max),
        options.rounding,
        rect_color,
    );

    let wide_enough_for_text = width > 32.0;
    if wide_enough_for_text {
        let rect_min = rect_min.max(info.canvas.min);
        let rect_max = rect_max.min(info.canvas.max);

        let painter = info
            .painter
            .sub_region(Rect::from_min_max(rect_min, rect_max));

        let duration_ms = to_ms(record.duration_ns);
        let text = if record.data.is_empty() {
            format!("{}{} {:6.3} ms", prefix, record.id, duration_ms)
        } else {
            format!(
                "{}{} {:?} {:6.3} ms",
                prefix, record.id, record.data, duration_ms
            )
        };
        let pos = pos2(
            start_x + 4.0,
            top_y + 0.5 * (options.rect_height - info.text_height),
        );
        let pos = painter.round_pos_to_pixels(pos);
        const TEXT_COLOR: Color32 = Color32::BLACK;
        painter.text(pos, Align2::LEFT_TOP, text, TEXT_STYLE, TEXT_COLOR);
    }

    if is_hovered {
        PaintResult::Hovered
    } else {
        PaintResult::Normal
    }
}

fn color_from_duration(ns: NanoSecond) -> Rgba {
    let ms = to_ms(ns) as f32;
    // Brighter = more time.
    // So we start with dark colors (blue) and later bright colors (green).
    let b = remap_clamp(ms, 0.0..=5.0, 1.0..=0.3);
    let r = remap_clamp(ms, 0.0..=10.0, 0.5..=0.8);
    let g = remap_clamp(ms, 10.0..=33.0, 0.1..=0.8);
    let a = 0.9;
    Rgba::from_rgb(r, g, b) * a
}

fn to_ms(ns: NanoSecond) -> f64 {
    ns as f64 * 1e-6
}

fn paint_scope(
    info: &Info,
    options: &mut Options,
    stream: &Stream,
    scope: &Scope<'_>,
    depth: usize,
    min_y: &mut f32,
) -> Result<PaintResult> {
    let top_y = info.canvas.max.y - (1.0 + depth as f32) * (options.rect_height + options.spacing);
    *min_y = min_y.min(top_y);

    let result = paint_record(info, options, "", &scope.record, top_y);

    if result != PaintResult::Culled {
        let mut num_children = 0;
        for child_scope in Reader::with_offset(stream, scope.child_begin_position)? {
            paint_scope(info, options, stream, &child_scope?, depth + 1, min_y)?;
            num_children += 1;
        }

        if result == PaintResult::Hovered {
            egui::show_tooltip_at_pointer(&info.ctx, Id::new("puffin_profiler_tooltip"), |ui| {
                ui.monospace(format!("id:       {}", scope.record.id));
                if !scope.record.location.is_empty() {
                    ui.monospace(format!("location: {}", scope.record.location));
                }
                if !scope.record.data.is_empty() {
                    ui.monospace(format!("data:     {}", scope.record.data));
                }
                ui.monospace(format!(
                    "duration: {:6.3} ms",
                    to_ms(scope.record.duration_ns)
                ));
                ui.monospace(format!("children: {}", num_children));
            });
        }
    }

    Ok(result)
}

fn paint_merge_scope(
    info: &Info,
    options: &mut Options,
    stream: &Stream,
    merge: &MergeScope<'_>,
    depth: usize,
    min_y: &mut f32,
) -> Result<PaintResult> {
    let top_y = info.canvas.max.y - (1.0 + depth as f32) * (options.rect_height + options.spacing);
    *min_y = min_y.min(top_y);

    let prefix = if merge.pieces.len() <= 1 {
        String::default()
    } else {
        format!("{}x ", merge.pieces.len())
    };
    let result = paint_record(info, options, &prefix, &merge.record, top_y);

    if result != PaintResult::Culled {
        for merged_child in merge_children_of_pieces(stream, merge)? {
            paint_merge_scope(info, options, stream, &merged_child, depth + 1, min_y)?;
        }

        if result == PaintResult::Hovered {
            egui::show_tooltip_at_pointer(&info.ctx, Id::new("puffin_profiler_tooltip"), |ui| {
                merge_scope_tooltip(ui, merge)
            });
        }
    }

    Ok(result)
}

fn merge_scope_tooltip(ui: &mut egui::Ui, merge: &MergeScope<'_>) {
    ui.monospace(format!("id:       {}", merge.record.id));
    if !merge.record.location.is_empty() {
        ui.monospace(format!("location: {}", merge.record.location));
    }
    if !merge.record.data.is_empty() {
        ui.monospace(format!("data:     {}", merge.record.data));
    }

    if merge.pieces.len() <= 1 {
        ui.monospace(format!(
            "duration: {:6.3} ms",
            to_ms(merge.record.duration_ns)
        ));
    } else {
        ui.monospace(format!("sum of:   {} scopes", merge.pieces.len()));
        ui.monospace(format!(
            "total:    {:6.3} ms",
            to_ms(merge.record.duration_ns)
        ));

        ui.monospace(format!(
            "mean:     {:6.3} ms",
            to_ms(merge.record.duration_ns) / (merge.pieces.len() as f64),
        ));
        let max_duration_ns = merge
            .pieces
            .iter()
            .map(|piece| piece.scope.record.duration_ns)
            .max()
            .unwrap();
        ui.monospace(format!("max:      {:6.3} ms", to_ms(max_duration_ns)));
    }
}

fn paint_thread_info(info: &Info, thread: &ThreadInfo, pos: Pos2) {
    let galley = info
        .ctx
        .fonts()
        .layout_single_line(TEXT_STYLE, thread.name.clone());
    let rect = Rect::from_min_size(pos, galley.size);

    info.painter
        .rect_filled(rect.expand(2.0), 0.0, Rgba::from_black_alpha(0.5));
    info.painter
        .galley(rect.min, galley, Rgba::from_white_alpha(0.9).into());
}

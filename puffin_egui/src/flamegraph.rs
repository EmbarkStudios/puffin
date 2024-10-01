use std::vec;

use super::{SelectedFrames, ERROR_COLOR, HOVER_COLOR};
use crate::filter::Filter;
use egui::*;
use indexmap::IndexMap;
use puffin::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum SortBy {
    Time,
    Name,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct Sorting {
    pub sort_by: SortBy,
    pub reversed: bool,
}

impl Default for Sorting {
    fn default() -> Self {
        Self {
            sort_by: SortBy::Time,
            reversed: false,
        }
    }
}

impl Sorting {
    fn sort(self, mut threads: Vec<ThreadInfo>) -> Vec<ThreadInfo> {
        match self.sort_by {
            SortBy::Time => {
                threads.sort_by_key(|info| info.start_time_ns);
            }
            SortBy::Name => {
                threads.sort_by(|a, b| natord::compare_ignore_case(&a.name, &b.name));
            }
        }
        if self.reversed {
            threads.reverse();
        }
        threads
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Sort threads by:");

            let dir = if self.reversed { '⬆' } else { '⬇' };

            for &sort_by in &[SortBy::Time, SortBy::Name] {
                let selected = self.sort_by == sort_by;

                let label = if selected {
                    format!("{sort_by:?} {dir}")
                } else {
                    format!("{sort_by:?}")
                };

                if ui.add(egui::RadioButton::new(selected, label)).clicked() {
                    if selected {
                        self.reversed = !self.reversed;
                    } else {
                        self.sort_by = sort_by;
                        self.reversed = false;
                    }
                }
            }
        });
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct ThreadVisualizationSettings {
    flamegraph_collapse: bool,
    flamegraph_show: bool,
}

impl Default for ThreadVisualizationSettings {
    fn default() -> Self {
        Self {
            flamegraph_collapse: false,
            flamegraph_show: true,
        }
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct Options {
    // --------------------
    // View:
    /// Controls zoom
    pub canvas_width_ns: f32,

    /// How much we have panned sideways:
    pub sideways_pan_in_points: f32,

    // --------------------
    // Visuals:
    /// Events shorter than this many points aren't painted
    pub cull_width: f32,
    /// Draw each item with at least this width (only makes sense if [`Self::cull_width`] is 0)
    pub min_width: f32,

    pub rect_height: f32,
    pub spacing: f32,
    pub rounding: f32,

    pub frame_list_height: f32,
    /// Distance between subsequent frames in the frame view.
    pub frame_width: f32,

    /// Aggregate child scopes with the same id?
    pub merge_scopes: bool,

    pub sorting: Sorting,

    /// Visual settings for threads.
    pub flamegraph_threads: IndexMap<String, ThreadVisualizationSettings>,

    /// Interval of vertical timeline indicators.
    grid_spacing_micros: f64,

    #[cfg_attr(feature = "serde", serde(skip))]
    scope_name_filter: Filter,

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

            // cull_width: 0.5, // save some CPU?
            cull_width: 0.0, // no culling
            min_width: 1.0,

            rect_height: 16.0,
            spacing: 4.0,
            rounding: 4.0,

            frame_list_height: 48.0,
            frame_width: 10.,

            merge_scopes: false, // off, because it really only works well for single-threaded profiling

            grid_spacing_micros: 1.,

            sorting: Default::default(),
            scope_name_filter: Default::default(),

            zoom_to_relative_ns_range: None,
            flamegraph_threads: IndexMap::new(),
        }
    }
}

/// Context for painting a frame.
struct Info<'a> {
    ctx: egui::Context,
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
    /// How many frames we are viewing
    num_frames: usize,
    /// LayerId to use as parent for tooltips
    layer_id: LayerId,

    font_id: FontId,

    scope_collection: &'a ScopeCollection,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PaintResult {
    Culled,
    Hovered,
    Normal,
}

impl<'a> Info<'a> {
    fn point_from_ns(&self, options: &Options, ns: NanoSecond) -> f32 {
        self.canvas.min.x
            + options.sideways_pan_in_points
            + self.canvas.width() * ((ns - self.start_ns) as f32) / options.canvas_width_ns
    }
}

/// Show the flamegraph.
pub fn ui(
    ui: &mut egui::Ui,
    options: &mut Options,
    scope_collection: &ScopeCollection,
    frames: &SelectedFrames,
) {
    puffin::profile_function!();
    let mut reset_view = false;

    let num_frames = frames.frames.len();

    {
        // reset view if number of selected frames changes (and we are viewing all of them):
        let num_frames_id = ui.id().with("num_frames");
        let num_frames_last_frame =
            ui.memory_mut(|m| m.data.get_temp::<usize>(num_frames_id).unwrap_or_default());

        if num_frames_last_frame != num_frames && !options.merge_scopes {
            reset_view = true;
        }
        ui.memory_mut(|m| m.data.insert_temp(num_frames_id, num_frames));
    }

    ui.horizontal(|ui| {
        options.scope_name_filter.ui(ui);

        ui.menu_button("🔧 Settings", |ui| {
            ui.set_max_height(500.0);

            {
                let changed = ui
                    .checkbox(&mut options.merge_scopes, "Merge children with same ID")
                    .changed();
                // If we have multiple frames selected this will toggle
                // if we view all the frames, or an average of them,
                // and that difference is pretty massive, so help the user:
                if changed && num_frames > 1 {
                    reset_view = true;
                }
            }

            ui.horizontal(|ui| {
                ui.label("Grid spacing:");
                let grid_spacing_drag = DragValue::new(&mut options.grid_spacing_micros)
                    .speed(0.1)
                    .range(1.0..=100.0)
                    .suffix(" µs");
                grid_spacing_drag.ui(ui);
            });

            // The number of threads can change between frames, so always show this even if there currently is only one thread:
            options.sorting.ui(ui);

            ui.group(|ui| {
                ui.strong("Visible Threads");
                egui::ScrollArea::vertical().id_salt("f").show(ui, |ui| {
                    for f in frames.threads.keys() {
                        let entry = options
                            .flamegraph_threads
                            .entry(f.name.clone())
                            .or_default();
                        ui.checkbox(&mut entry.flamegraph_show, f.name.clone());
                    }
                });
            });
        });

        ui.menu_button("❓", |ui| {
            ui.label(
                "Drag to pan.\n\
                        Zoom: Ctrl/cmd + scroll, or drag with secondary mouse button.\n\
                        Click on a scope to zoom to it.\n\
                        Double-click to reset view.\n\
                        Press spacebar to pause/resume.",
            );
        });
    });

    Frame::dark_canvas(ui.style()).show(ui, |ui| {
        ui.visuals_mut().clip_rect_margin = 0.0;

        let available_height = ui.max_rect().bottom() - ui.min_rect().bottom();
        ScrollArea::vertical().show(ui, |ui| {
            let mut canvas = ui.available_rect_before_wrap();
            canvas.max.y = f32::INFINITY;
            let response = ui.interact(canvas, ui.id().with("canvas"), Sense::click_and_drag());

            let (min_ns, max_ns) = if options.merge_scopes {
                frames.merged_range_ns
            } else {
                frames.raw_range_ns
            };

            let info = Info {
                ctx: ui.ctx().clone(),
                canvas,
                response,
                painter: ui.painter_at(canvas),
                text_height: 15.0, // TODO
                start_ns: min_ns,
                stop_ns: max_ns,
                num_frames: frames.frames.len(),
                layer_id: ui.layer_id(),
                font_id: TextStyle::Body.resolve(ui.style()),
                scope_collection,
            };

            if reset_view {
                options.zoom_to_relative_ns_range = Some((
                    info.ctx.input(|i| i.time),
                    (0, info.stop_ns - info.start_ns),
                ));
            }

            interact_with_canvas(options, &info.response, &info);

            let where_to_put_timeline = info.painter.add(Shape::Noop);

            let max_y = ui_canvas(options, &info, frames, (min_ns, max_ns));

            let mut used_rect = canvas;
            used_rect.max.y = max_y;

            // Fill out space that we don't use so that the `ScrollArea` doesn't collapse in height:
            used_rect.max.y = used_rect.max.y.max(used_rect.min.y + available_height);

            let timeline = paint_timeline(&info, used_rect, options, min_ns);
            info.painter
                .set(where_to_put_timeline, Shape::Vec(timeline));

            ui.allocate_rect(used_rect, Sense::hover());
        });
    });
}

fn ui_canvas(
    options: &mut Options,
    info: &Info<'_>,
    frames: &SelectedFrames,
    (min_ns, max_ns): (NanoSecond, NanoSecond),
) -> f32 {
    puffin::profile_function!();

    if options.canvas_width_ns <= 0.0 {
        options.canvas_width_ns = (max_ns - min_ns) as f32;
        options.zoom_to_relative_ns_range = None;
    }

    // We paint the threads top-down
    let mut cursor_y = info.canvas.top();
    cursor_y += info.text_height; // Leave room for time labels

    let threads = frames.threads.keys().cloned().collect();
    let threads = options.sorting.sort(threads);

    for thread_info in threads {
        let thread_visualization = options
            .flamegraph_threads
            .entry(thread_info.name.clone())
            .or_default();

        if !thread_visualization.flamegraph_show {
            continue;
        }

        // Visual separator between threads:
        cursor_y += 2.0;
        let line_y = cursor_y;
        cursor_y += 2.0;

        let text_pos = pos2(info.canvas.min.x, cursor_y);

        paint_thread_info(
            info,
            &thread_info,
            text_pos,
            &mut thread_visualization.flamegraph_collapse,
        );

        // draw on top of thread info background:
        info.painter.line_segment(
            [
                pos2(info.canvas.min.x, line_y),
                pos2(info.canvas.max.x, line_y),
            ],
            Stroke::new(1.0, Rgba::from_white_alpha(0.5)),
        );

        cursor_y += info.text_height;

        if !thread_visualization.flamegraph_collapse {
            let mut paint_streams = || -> Result<()> {
                if options.merge_scopes {
                    for merge in &frames.threads[&thread_info].merged_scopes {
                        paint_merge_scope(info, options, 0, merge, 0, cursor_y);
                    }
                } else {
                    for stream_info in &frames.threads[&thread_info].streams {
                        let top_scopes =
                            Reader::from_start(&stream_info.stream).read_top_scopes()?;
                        for scope in top_scopes {
                            paint_scope(info, options, &stream_info.stream, &scope, 0, cursor_y)?;
                        }
                    }
                }
                Ok(())
            };

            if let Err(err) = paint_streams() {
                let text = format!("Profiler stream error: {err:?}");
                info.painter.text(
                    pos2(info.canvas.min.x, cursor_y),
                    Align2::LEFT_TOP,
                    text,
                    info.font_id.clone(),
                    ERROR_COLOR,
                );
            }

            let max_depth = frames.threads[&thread_info].max_depth;
            cursor_y += max_depth as f32 * (options.rect_height + options.spacing);
        }
        cursor_y += info.text_height; // Extra spacing between threads
    }

    cursor_y
}

fn interact_with_canvas(options: &mut Options, response: &Response, info: &Info<'_>) {
    if response.drag_delta().x != 0.0 {
        options.sideways_pan_in_points += response.drag_delta().x;
        options.zoom_to_relative_ns_range = None;
    }

    if response.hovered() {
        // Sideways pan with e.g. a touch pad:
        if info.ctx.input(|i| i.smooth_scroll_delta.x != 0.0) {
            options.sideways_pan_in_points += info.ctx.input(|i| i.smooth_scroll_delta.x);
            options.zoom_to_relative_ns_range = None;
        }

        let mut zoom_factor = info.ctx.input(|i| i.zoom_delta_2d().x);

        if response.dragged_by(PointerButton::Secondary) {
            zoom_factor *= (response.drag_delta().y * 0.01).exp();
        }

        if zoom_factor != 1.0 {
            options.canvas_width_ns /= zoom_factor;

            if let Some(mouse_pos) = response.hover_pos() {
                let zoom_center = mouse_pos.x - info.canvas.min.x;
                options.sideways_pan_in_points =
                    (options.sideways_pan_in_points - zoom_center) * zoom_factor + zoom_center;
            }
            options.zoom_to_relative_ns_range = None;
        }
    }

    if response.double_clicked() {
        // Reset view
        options.zoom_to_relative_ns_range = Some((
            info.ctx.input(|i| i.time),
            (0, info.stop_ns - info.start_ns),
        ));
    }

    if let Some((start_time, (start_ns, end_ns))) = options.zoom_to_relative_ns_range {
        const ZOOM_DURATION: f32 = 0.75;
        let t = (info.ctx.input(|i| i.time - start_time) as f32 / ZOOM_DURATION).min(1.0);

        let canvas_width = response.rect.width();

        let target_canvas_width_ns = (end_ns - start_ns) as f32;
        let target_pan_in_points = -canvas_width * start_ns as f32 / target_canvas_width_ns;

        options.canvas_width_ns = lerp(
            options.canvas_width_ns.recip()..=target_canvas_width_ns.recip(),
            t,
        )
        .recip();
        options.sideways_pan_in_points =
            lerp(options.sideways_pan_in_points..=target_pan_in_points, t);

        if t >= 1.0 {
            options.zoom_to_relative_ns_range = None;
        }

        info.ctx.request_repaint();
    }
}

fn paint_timeline(
    info: &Info<'_>,
    canvas: Rect,
    options: &Options,
    start_ns: NanoSecond,
) -> Vec<egui::Shape> {
    let mut shapes = vec![];

    if options.canvas_width_ns <= 0.0 {
        return shapes;
    }

    let alpha_multiplier = if options.scope_name_filter.is_empty() {
        0.3
    } else {
        0.1
    };

    // We show all measurements relative to start_ns

    let max_lines = canvas.width() / 4.0;
    let mut grid_spacing_ns = (options.grid_spacing_micros * 1_000.) as i64;
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
        let line_x = info.point_from_ns(options, start_ns + grid_ns);
        if line_x > canvas.max.x {
            break;
        }

        if canvas.min.x <= line_x {
            let big_line = grid_ns % (grid_spacing_ns * 100) == 0;
            let medium_line = grid_ns % (grid_spacing_ns * 10) == 0;

            let line_alpha = if big_line {
                big_alpha
            } else if medium_line {
                medium_alpha
            } else {
                tiny_alpha
            };

            shapes.push(egui::Shape::line_segment(
                [pos2(line_x, canvas.min.y), pos2(line_x, canvas.max.y)],
                Stroke::new(1.0, Rgba::from_white_alpha(line_alpha * alpha_multiplier)),
            ));

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

                info.painter.fonts(|f| {
                    // Text at top:
                    shapes.push(egui::Shape::text(
                        f,
                        pos2(text_x, canvas.min.y),
                        Align2::LEFT_TOP,
                        &text,
                        info.font_id.clone(),
                        text_color,
                    ));
                });

                info.painter.fonts(|f| {
                    // Text at bottom:
                    shapes.push(egui::Shape::text(
                        f,
                        pos2(text_x, canvas.max.y - info.text_height),
                        Align2::LEFT_TOP,
                        &text,
                        info.font_id.clone(),
                        text_color,
                    ));
                });
            }
        }

        grid_ns += grid_spacing_ns;
    }

    shapes
}

fn grid_text(grid_ns: NanoSecond) -> String {
    let grid_ms = to_ms(grid_ns);
    if grid_ns % 1_000_000 == 0 {
        format!("{grid_ms:.0} ms")
    } else if grid_ns % 100_000 == 0 {
        format!("{grid_ms:.1} ms")
    } else if grid_ns % 10_000 == 0 {
        format!("{grid_ms:.2} ms")
    } else {
        format!("{grid_ms:.3} ms")
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_record(
    info: &Info<'_>,
    options: &mut Options,
    prefix: &str,
    suffix: &str,
    scope_id: ScopeId,
    scope_data: &ScopeRecord<'_>,
    top_y: f32,
) -> PaintResult {
    let start_x = info.point_from_ns(options, scope_data.start_ns);
    let stop_x = info.point_from_ns(options, scope_data.stop_ns());
    if info.canvas.max.x < start_x
        || stop_x < info.canvas.min.x
        || stop_x - start_x < options.cull_width
    {
        return PaintResult::Culled;
    }

    let bottom_y = top_y + options.rect_height;

    let rect = Rect::from_min_max(pos2(start_x, top_y), pos2(stop_x, bottom_y));

    let is_hovered = if let Some(mouse_pos) = info.response.hover_pos() {
        rect.contains(mouse_pos)
    } else {
        false
    };

    let Some(scope_details) = info.scope_collection.fetch_by_id(&scope_id) else {
        return PaintResult::Culled;
    };

    if info.response.double_clicked() {
        if let Some(mouse_pos) = info.response.interact_pointer_pos() {
            if rect.contains(mouse_pos) {
                options
                    .scope_name_filter
                    .set_filter(scope_details.name().to_string());
            }
        }
    } else if is_hovered && info.response.clicked() {
        options.zoom_to_relative_ns_range = Some((
            info.ctx.input(|i| i.time),
            (
                scope_data.start_ns - info.start_ns,
                scope_data.stop_ns() - info.start_ns,
            ),
        ));
    }

    let mut rect_color = if is_hovered {
        HOVER_COLOR
    } else {
        color_from_duration(scope_data.duration_ns)
    };

    let mut min_width = options.min_width;

    if !options.scope_name_filter.is_empty() {
        if options.scope_name_filter.include(scope_details.name()) {
            // keep full opacity
            min_width *= 2.0; // make it more visible even when thin
        } else {
            // fade to highlight others
            rect_color = lerp(Rgba::BLACK..=rect_color, 0.075);
        }
    }

    if rect.width() <= min_width {
        // faster to draw it as a thin line
        info.painter.line_segment(
            [rect.center_top(), rect.center_bottom()],
            egui::Stroke::new(min_width, rect_color),
        );
    } else {
        info.painter.rect_filled(rect, options.rounding, rect_color);
    }

    let wide_enough_for_text = stop_x - start_x > 32.0;
    if wide_enough_for_text {
        let painter = info.painter.with_clip_rect(rect.intersect(info.canvas));

        let scope_name = scope_details.name();

        let duration_ms = to_ms(scope_data.duration_ns);
        let text = if scope_data.data.is_empty() {
            format!(
                "{}{} {:6.3} ms {}",
                prefix,
                scope_name.as_str(),
                duration_ms,
                suffix
            )
        } else {
            // Note: we don't escape the scope data (`{:?}`), because that often leads to ugly extra backslashes.
            format!(
                "{}{} '{}' {:6.3} ms {}",
                prefix,
                scope_name.as_str(),
                scope_data.data,
                duration_ms,
                suffix
            )
        };
        let pos = pos2(
            start_x + 4.0,
            top_y + 0.5 * (options.rect_height - info.text_height),
        );
        let pos = painter.round_pos_to_pixels(pos);
        const TEXT_COLOR: Color32 = Color32::BLACK;
        painter.text(
            pos,
            Align2::LEFT_TOP,
            text,
            info.font_id.clone(),
            TEXT_COLOR,
        );
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
    info: &Info<'_>,
    options: &mut Options,
    stream: &Stream,
    scope: &Scope<'_>,
    depth: usize,
    min_y: f32,
) -> Result<PaintResult> {
    let top_y = min_y + (depth as f32) * (options.rect_height + options.spacing);

    let result = paint_record(info, options, "", "", scope.id, &scope.record, top_y);

    if result != PaintResult::Culled {
        let mut num_children = 0;
        for child_scope in Reader::with_offset(stream, scope.child_begin_position)? {
            paint_scope(info, options, stream, &child_scope?, depth + 1, min_y)?;
            num_children += 1;
        }

        if result == PaintResult::Hovered {
            let Some(scope_details) = info.scope_collection.fetch_by_id(&scope.id) else {
                return Ok(PaintResult::Culled);
            };
            egui::show_tooltip_at_pointer(
                &info.ctx,
                info.layer_id,
                Id::new("puffin_profiler_tooltip"),
                |ui| {
                    paint_scope_details(ui, scope.id, scope.record.data, scope_details);

                    ui.monospace(format!(
                        "duration: {:7.3} ms",
                        to_ms(scope.record.duration_ns)
                    ));
                    ui.monospace(format!("children: {num_children:3}"));
                },
            );
        }
    }

    Ok(result)
}

fn paint_merge_scope(
    info: &Info<'_>,
    options: &mut Options,
    ns_offset: NanoSecond,
    merge: &MergeScope<'_>,
    depth: usize,
    min_y: f32,
) -> PaintResult {
    let top_y = min_y + (depth as f32) * (options.rect_height + options.spacing);

    let prefix = if info.num_frames <= 1 {
        if merge.num_pieces <= 1 {
            String::default()
        } else {
            format!("{}x ", merge.num_pieces)
        }
    } else {
        let is_integral = merge.num_pieces % info.num_frames == 0;
        if is_integral {
            format!("{}x ", merge.num_pieces / info.num_frames)
        } else {
            format!("{:.2}x ", merge.num_pieces as f64 / info.num_frames as f64)
        }
    };

    let suffix = if info.num_frames <= 1 {
        ""
    } else {
        "per frame"
    };

    let record = ScopeRecord {
        start_ns: ns_offset + merge.relative_start_ns,
        duration_ns: merge.duration_per_frame_ns,
        data: &merge.data,
    };

    let result = paint_record(info, options, &prefix, suffix, merge.id, &record, top_y);

    if result != PaintResult::Culled {
        for child in &merge.children {
            paint_merge_scope(info, options, record.start_ns, child, depth + 1, min_y);
        }

        if result == PaintResult::Hovered {
            egui::show_tooltip_at_pointer(
                &info.ctx,
                info.layer_id,
                Id::new("puffin_profiler_tooltip"),
                |ui| {
                    merge_scope_tooltip(ui, info.scope_collection, merge, info.num_frames);
                },
            );
        }
    }

    result
}

fn paint_scope_details(ui: &mut Ui, scope_id: ScopeId, data: &str, scope_details: &ScopeDetails) {
    egui::Grid::new("scope_details_tooltip")
        .num_columns(2)
        .show(ui, |ui| {
            ui.monospace("id");
            ui.monospace(format!("{}", scope_id.0));
            ui.end_row();

            ui.monospace("function name");
            ui.monospace(scope_details.function_name.as_str());
            ui.end_row();

            if let Some(scope_name) = &scope_details.scope_name {
                ui.monospace("scope name");
                ui.monospace(scope_name.as_str());
                ui.end_row();
            }

            if !scope_details.file_path.is_empty() {
                ui.monospace("location");
                ui.monospace(scope_details.location());
                ui.end_row();
            }

            if !data.is_empty() {
                ui.monospace("data");
                ui.monospace(data.as_str());
                ui.end_row();
            }

            ui.monospace("scope type");
            ui.monospace(scope_details.scope_type().type_str());
            ui.end_row();
        });
}

fn merge_scope_tooltip(
    ui: &mut egui::Ui,
    scope_collection: &ScopeCollection,
    merge: &MergeScope<'_>,
    num_frames: usize,
) {
    #![allow(clippy::collapsible_else_if)]

    let Some(scope_details) = scope_collection.fetch_by_id(&merge.id) else {
        return;
    };

    paint_scope_details(ui, merge.id, &merge.data, scope_details);

    if num_frames <= 1 {
        if merge.num_pieces <= 1 {
            ui.monospace(format!(
                "duration: {:7.3} ms",
                to_ms(merge.duration_per_frame_ns)
            ));
        } else {
            ui.monospace(format!("sum of {} scopes", merge.num_pieces));
            ui.monospace(format!(
                "total: {:7.3} ms",
                to_ms(merge.duration_per_frame_ns)
            ));
            ui.monospace(format!(
                "mean:  {:7.3} ms",
                to_ms(merge.duration_per_frame_ns) / (merge.num_pieces as f64),
            ));
            ui.monospace(format!("max:   {:7.3} ms", to_ms(merge.max_duration_ns)));
        }
    } else {
        ui.monospace(format!(
            "{} calls over all {} frames",
            merge.num_pieces, num_frames
        ));

        if merge.num_pieces == num_frames {
            ui.monospace("1 call / frame");
        } else if merge.num_pieces % num_frames == 0 {
            ui.monospace(format!("{} calls / frame", merge.num_pieces / num_frames));
        } else {
            ui.monospace(format!(
                "{:.3} calls / frame",
                merge.num_pieces as f64 / num_frames as f64
            ));
        }

        ui.monospace(format!(
            "{:7.3} ms / frame",
            to_ms(merge.duration_per_frame_ns)
        ));
        ui.monospace(format!(
            "{:7.3} ms / call",
            to_ms(merge.total_duration_ns) / (merge.num_pieces as f64),
        ));
        ui.monospace(format!(
            "{:7.3} ms for slowest call",
            to_ms(merge.max_duration_ns)
        ));
    }
}

fn paint_thread_info(info: &Info<'_>, thread: &ThreadInfo, pos: Pos2, collapsed: &mut bool) {
    let collapsed_symbol = if *collapsed { "⏵" } else { "⏷" };

    let galley = info.ctx.fonts(|f| {
        f.layout_no_wrap(
            format!("{} {}", collapsed_symbol, thread.name.clone()),
            info.font_id.clone(),
            egui::Color32::PLACEHOLDER,
        )
    });

    let rect = Rect::from_min_size(pos, galley.size());

    let is_hovered = if let Some(mouse_pos) = info.response.hover_pos() {
        rect.contains(mouse_pos)
    } else {
        false
    };

    let text_color = if is_hovered {
        Color32::WHITE
    } else {
        Color32::from_white_alpha(229)
    };
    let back_color = if is_hovered {
        Color32::from_black_alpha(100)
    } else {
        Color32::BLACK
    };

    info.painter.rect_filled(rect.expand(2.0), 0.0, back_color);
    info.painter.galley(rect.min, galley, text_color);

    if is_hovered && info.response.clicked() {
        *collapsed = !(*collapsed);
    }
}

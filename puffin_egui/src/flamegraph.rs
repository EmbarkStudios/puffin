use super::{ERROR_COLOR, HOVER_COLOR};
use egui::*;
use puffin::*;
use std::sync::Arc;

const TEXT_STYLE: TextStyle = TextStyle::Body;

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum SortBy {
    Time,
    Name,
}

#[derive(Clone, Copy, Debug, PartialEq)]
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
                    format!("{:?} {}", sort_by, dir)
                } else {
                    format!("{:?}", sort_by)
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

#[derive(Clone, Copy, Debug)]
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

    /// Aggregate child scopes with the same id?
    pub merge_scopes: bool,

    pub sorting: Sorting,

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
            min_width: 0.5,

            rect_height: 16.0,
            spacing: 4.0,
            rounding: 4.0,

            frame_list_height: 48.0,

            merge_scopes: true,

            sorting: Default::default(),

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

/// Show the flamegraph.
pub fn ui(ui: &mut egui::Ui, options: &mut Options, frames: &vec1::Vec1<Arc<FrameData>>) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut options.merge_scopes, "Merge children with same ID");
        ui.separator();
        ui.add(Label::new("Help!").text_color(ui.visuals().widgets.inactive.text_color()))
            .on_hover_text(
                "Drag to pan.\n\
            Zoom: Ctrl/cmd + scroll, or drag with secondary mouse button.\n\
            Click on a scope to zoom to it.\n\
            Double-click to reset view.\n\
            Press spacebar to pause/resume.",
            );
        ui.separator();
        // The number of threads can change between frames, so always show this even if there currently is only one thread:
        options.sorting.ui(ui);
    });

    Frame::dark_canvas(ui.style()).show(ui, |ui| {
        let available_height = ui.max_rect().bottom() - ui.min_rect().bottom();
        ScrollArea::auto_sized().show(ui, |ui| {
            let canvas = ui.available_rect_before_wrap();
            let response = ui.interact(canvas, ui.id(), Sense::click_and_drag());

            let min_ns = frames.first().range_ns.0;
            let max_ns = frames.last().range_ns.1;
            let info = Info {
                ctx: ui.ctx().clone(),
                canvas,
                response,
                painter: ui.painter_at(canvas),
                text_height: 15.0, // TODO
                start_ns: min_ns,
                stop_ns: max_ns,
            };
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

            ui.allocate_rect(used_rect, Sense::click_and_drag());
        });
    });
}

fn ui_canvas(
    options: &mut Options,
    info: &Info,
    frames: &vec1::Vec1<Arc<FrameData>>,
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

    let threads = frames
        .iter()
        .flat_map(|f| f.thread_streams.keys().cloned())
        .collect();
    let threads = options.sorting.sort(threads);

    for thread_info in threads {
        // Visual separator between threads:
        cursor_y += 2.0;
        let line_y = cursor_y;
        cursor_y += 2.0;

        let text_pos = pos2(info.canvas.min.x, cursor_y);
        paint_thread_info(info, &thread_info, text_pos);
        cursor_y += info.text_height;

        // draw on top of thread info background:
        info.painter.line_segment(
            [
                pos2(info.canvas.min.x, line_y),
                pos2(info.canvas.max.x, line_y),
            ],
            Stroke::new(1.0, Rgba::from_white_alpha(0.5)),
        );

        let mut paint_streams = || -> Result<()> {
            let streams = frames
                .iter()
                .filter_map(|frame| frame.thread_streams.get(&thread_info))
                .map(|stream_info| &stream_info.stream);
            if options.merge_scopes {
                let merges = puffin::merge_scopes_in_streams(streams)?;
                for merge in merges {
                    paint_merge_scope(info, options, 0, &merge, 0, cursor_y)?;
                }
            } else {
                for stream in streams {
                    let top_scopes = Reader::from_start(stream).read_top_scopes()?;
                    for scope in top_scopes {
                        paint_scope(info, options, stream, &scope, 0, cursor_y)?;
                    }
                }
            }
            Ok(())
        };

        if let Err(err) = paint_streams() {
            let text = format!("Profiler stream error: {:?}", err);
            info.painter.text(
                pos2(info.canvas.min.x, cursor_y),
                Align2::LEFT_TOP,
                text,
                TEXT_STYLE,
                ERROR_COLOR,
            );
        }

        let mut max_depth = 0;
        for stream_info in frames
            .iter()
            .filter_map(|frame| frame.thread_streams.get(&thread_info))
        {
            max_depth = stream_info.depth.max(max_depth);
        }

        cursor_y += max_depth as f32 * (options.rect_height + options.spacing);

        cursor_y += info.text_height; // Extra spacing between threads
    }

    cursor_y
}

fn interact_with_canvas(options: &mut Options, response: &Response, info: &Info) {
    if response.drag_delta().x != 0.0 {
        options.sideways_pan_in_points += response.drag_delta().x;
        options.zoom_to_relative_ns_range = None;
    }

    if response.hovered() {
        // Sideways pan with e.g. a touch pad:
        if info.ctx.input().scroll_delta.x != 0.0 {
            options.sideways_pan_in_points += info.ctx.input().scroll_delta.x;
            options.zoom_to_relative_ns_range = None;
        }

        let mut zoom_factor = info.ctx.input().zoom_delta_2d().x;

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
        options.zoom_to_relative_ns_range =
            Some((info.ctx.input().time, (0, info.stop_ns - info.start_ns)));
    }

    if let Some((start_time, (start_ns, end_ns))) = options.zoom_to_relative_ns_range {
        const ZOOM_DURATION: f32 = 0.75;
        let t = ((info.ctx.input().time - start_time) as f32 / ZOOM_DURATION).min(1.0);

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
    info: &Info,
    canvas: Rect,
    options: &Options,
    start_ns: NanoSecond,
) -> Vec<egui::Shape> {
    let mut shapes = vec![];

    if options.canvas_width_ns <= 0.0 {
        return shapes;
    }

    // We show all measurements relative to start_ns

    let max_lines = canvas.width() / 4.0;
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
                Stroke::new(1.0, Rgba::from_white_alpha(line_alpha)),
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

                // Text at top:
                shapes.push(egui::Shape::text(
                    info.painter.fonts(),
                    pos2(text_x, canvas.min.y),
                    Align2::LEFT_TOP,
                    &text,
                    TEXT_STYLE,
                    text_color,
                ));

                // Text at bottom:
                shapes.push(egui::Shape::text(
                    info.painter.fonts(),
                    pos2(text_x, canvas.max.y - info.text_height),
                    Align2::LEFT_TOP,
                    &text,
                    TEXT_STYLE,
                    text_color,
                ));
            }
        }

        grid_ns += grid_spacing_ns;
    }

    shapes
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
    let mut start_x = info.point_from_ns(options, record.start_ns);
    let mut stop_x = info.point_from_ns(options, record.stop_ns());
    if info.canvas.max.x < start_x
        || stop_x < info.canvas.min.x
        || stop_x - start_x < options.cull_width
    {
        return PaintResult::Culled;
    }

    if stop_x - start_x < options.min_width {
        // Make sure it is visible:
        let center = 0.5 * (start_x + stop_x);
        start_x = center - 0.5 * options.min_width;
        stop_x = center + 0.5 * options.min_width;
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

    let wide_enough_for_text = stop_x - start_x > 32.0;
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
    min_y: f32,
) -> Result<PaintResult> {
    let top_y = min_y + (depth as f32) * (options.rect_height + options.spacing);

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
    ns_offset: NanoSecond,
    merge: &MergeScope<'_>,
    depth: usize,
    min_y: f32,
) -> Result<PaintResult> {
    let top_y = min_y + (depth as f32) * (options.rect_height + options.spacing);

    let prefix = if merge.num_pieces <= 1 {
        String::default()
    } else {
        format!("{}x ", merge.num_pieces)
    };

    let record = Record {
        start_ns: ns_offset + merge.relative_start_ns,
        duration_ns: merge.total_duration_ns,
        id: merge.id,
        location: merge.location,
        data: merge.data,
    };

    let result = paint_record(info, options, &prefix, &record, top_y);

    if result != PaintResult::Culled {
        for child in &merge.children {
            paint_merge_scope(info, options, record.start_ns, &child, depth + 1, min_y)?;
        }

        if result == PaintResult::Hovered {
            egui::show_tooltip_at_pointer(&info.ctx, Id::new("puffin_profiler_tooltip"), |ui| {
                merge_scope_tooltip(ui, merge);
            });
        }
    }

    Ok(result)
}

fn merge_scope_tooltip(ui: &mut egui::Ui, merge: &MergeScope<'_>) {
    ui.monospace(format!("id:       {}", merge.id));
    if !merge.location.is_empty() {
        ui.monospace(format!("location: {}", merge.location));
    }
    if !merge.data.is_empty() {
        ui.monospace(format!("data:     {}", merge.data));
    }

    if merge.num_pieces <= 1 {
        ui.monospace(format!(
            "duration: {:6.3} ms",
            to_ms(merge.total_duration_ns)
        ));
    } else {
        ui.monospace(format!("sum of:   {} scopes", merge.num_pieces));
        ui.monospace(format!(
            "total:    {:6.3} ms",
            to_ms(merge.total_duration_ns)
        ));
        ui.monospace(format!(
            "mean:     {:6.3} ms",
            to_ms(merge.total_duration_ns) / (merge.num_pieces as f64),
        ));
        ui.monospace(format!("max:      {:6.3} ms", to_ms(merge.max_duration_ns)));
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

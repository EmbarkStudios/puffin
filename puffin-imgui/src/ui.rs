use imgui::*;
use puffin::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn min(self, other: Self) -> Self {
        Self {
            x: self.x.min(other.x),
            y: self.y.min(other.y),
        }
    }

    pub fn max(self, other: Self) -> Self {
        Self {
            x: self.x.max(other.x),
            y: self.y.max(other.y),
        }
    }
}

impl From<[f32; 2]> for Vec2 {
    fn from(v: [f32; 2]) -> Self {
        Self::new(v[0], v[1])
    }
}

impl From<Vec2> for [f32; 2] {
    fn from(v: Vec2) -> Self {
        [v.x, v.y]
    }
}

impl std::ops::Add<Vec2> for Vec2 {
    type Output = Vec2;
    fn add(self, rhs: Vec2) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

// ----------------------------------------------------------------------------

const ERROR_COLOR: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const HOVER_COLOR: [f32; 4] = [0.85, 0.85, 0.85, 1.0];

/// The frames we can select between
#[derive(Clone)]
pub struct Frames {
    pub recent: Vec<Arc<FrameData>>,
    pub slowest: Vec<Arc<FrameData>>,
}

#[derive(Clone)]
enum Selected {
    Latest,
    Specific(Arc<FrameData>),
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ProfilerUi {
    options: Options,

    // interaction:
    #[serde(skip)]
    is_panning: bool,

    #[serde(skip)]
    selected: Selected,

    /// If `None`, we show the latest frames
    #[serde(skip)]
    paused_frames: Option<Frames>,
}

impl Default for ProfilerUi {
    fn default() -> Self {
        Self {
            options: Options::default(),
            is_panning: false,
            selected: Selected::Latest,
            paused_frames: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
struct Options {
    // --------------------
    // View:
    /// Controls zoom
    canvas_width_ns: f32,

    /// How much we have panned sideways:
    sideways_pan_in_pixels: f32,

    // --------------------
    // Interact:
    scroll_zoom_speed: f32,

    // --------------------
    // Visuals:
    /// Events shorter than this many pixels aren't painted
    cull_width: f32,
    rect_height: f32,
    spacing: f32,
    rounding: f32,

    /// Aggregate child scopes with the same id?
    merge_scopes: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            canvas_width_ns: 0.0,
            sideways_pan_in_pixels: 0.0,

            scroll_zoom_speed: 0.01,

            cull_width: 0.5,
            rect_height: 16.0,
            spacing: 4.0,
            rounding: 4.0,

            merge_scopes: true,
        }
    }
}

struct Info<'ui> {
    // Bounding box of canvas in pixels:
    canvas_min: Vec2,
    canvas_max: Vec2,

    mouse_pos: Vec2,

    ui: &'ui Ui<'ui>,
    draw_list: &'ui DrawListMut<'ui>,
    font_size: f32,

    /// Time of first event
    start_ns: NanoSecond,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PaintResult {
    Culled,
    Hovered,
    Normal,
}

impl<'ui> Info<'ui> {
    fn canvas_width(&self) -> f32 {
        self.canvas_max.x - self.canvas_min.x
    }

    fn pixel_from_ns(&self, options: &Options, ns: NanoSecond) -> f32 {
        self.canvas_min.x
            + options.sideways_pan_in_pixels
            + self.canvas_width() * ((ns - self.start_ns) as f32) / options.canvas_width_ns
    }
}

impl ProfilerUi {
    /// Show a [`imgui::Window`] with the profiler contents.
    /// If you want to control the window yourself, use [`Self::ui`] instead.
    pub fn window(&mut self, ui: &Ui<'_>) -> bool {
        let mut open = true;
        imgui::Window::new(im_str!("Profiler"))
            .position([10.0, 25.0], Condition::FirstUseEver)
            .size([600.0, 250.0], Condition::FirstUseEver)
            .bg_alpha(0.99) // Transparency can be distracting
            .always_auto_resize(false)
            .opened(&mut open)
            .build(ui, || self.ui(ui));
        open
    }

    /// The frames we can select between
    fn frames(&self) -> Frames {
        self.paused_frames.clone().unwrap_or_else(|| {
            let profiler = GlobalProfiler::lock();
            Frames {
                recent: profiler.recent_frames().cloned().collect(),
                slowest: profiler.slowest_frames_chronological(),
            }
        })
    }

    fn selected_frame(&self) -> Option<Arc<FrameData>> {
        match &self.selected {
            Selected::Latest => GlobalProfiler::lock().latest_frame(),
            Selected::Specific(frame) => Some(frame.clone()),
        }
    }

    fn selected_frame_index(&self) -> Option<FrameIndex> {
        self.selected_frame().map(|frame| frame.frame_index)
    }

    /// Show the profiler.
    ///
    /// Call this from within an [`imgui::Window`], or use [`Self::window`] instead.
    pub fn ui(&mut self, ui: &Ui<'_>) {
        #![allow(clippy::collapsible_else_if)]

        if !puffin::are_scopes_on() {
            ui.text_colored(ERROR_COLOR, im_str!("The puffin profiler is OFF!"));
        }

        if self.paused_frames.is_none() {
            // showing latest data
            if ui.button(im_str!("Pause"), Default::default()) {
                self.paused_frames = Some(self.frames());
                if matches!(&self.selected, Selected::Latest) {
                    if let Some(latest) = GlobalProfiler::lock().latest_frame() {
                        self.selected = Selected::Specific(latest);
                    }
                }
            }
            ui.same_line(0.0);
            if ui.button(im_str!("Clear slowest frames"), Default::default()) {
                GlobalProfiler::lock().clear_slowest();
                self.paused_frames = None;
            }
        } else {
            if ui.button(im_str!("Resume"), Default::default()) {
                self.paused_frames = None;
            }
        }
        let mut hovered_frame = None;
        if imgui::CollapsingHeader::new(im_str!("Frames"))
            .default_open(true)
            .build(ui)
        {
            ui.indent();
            hovered_frame = self.show_frames(ui);

            match &self.selected {
                Selected::Latest => {
                    ui.text("Latest frame selected");
                }
                Selected::Specific(_frame) => {
                    if ui.button(im_str!("Show latest frame"), Default::default()) {
                        self.selected = Selected::Latest;
                        self.paused_frames = None;
                    }
                    if ui.is_item_hovered() {
                        hovered_frame = GlobalProfiler::lock().latest_frame();
                    }
                }
            }
            ui.unindent();
        }

        let frame = match hovered_frame.or_else(|| self.selected_frame()) {
            Some(frame) => frame,
            None => {
                ui.text("No profiling data");
                return;
            }
        };

        // TODO: show age of data

        let (min_ns, max_ns) = frame.range_ns;

        ui.text(im_str!(
            "Current frame: {:.1} ms, {} threads, {} scopes, {:.1} kB",
            (max_ns - min_ns) as f64 * 1e-6,
            frame.thread_streams.len(),
            frame.num_scopes,
            frame.num_bytes as f64 * 1e-3
        ));

        ui.text("Drag to pan. Scroll to zoom.");
        ui.same_line(0.0);
        if ui.button(im_str!("Reset view"), Default::default()) {
            self.options.canvas_width_ns = 0.0;
            self.options.sideways_pan_in_pixels = 0.0;
        }
        ui.same_line(0.0);
        ui.checkbox(
            im_str!("Merge children with same ID"),
            &mut self.options.merge_scopes,
        );
        ui.separator();

        let content_min: Vec2 = ui.cursor_screen_pos().into();
        let content_region_avail: Vec2 = ui.content_region_avail().into();
        let content_max = content_min + content_region_avail;

        let draw_list = ui.get_window_draw_list();

        draw_list.with_clip_rect_intersect(content_min.into(), content_max.into(), || {
            let mut info = Info {
                start_ns: min_ns,
                canvas_min: content_min,
                canvas_max: content_max,
                mouse_pos: ui.io().mouse_pos.into(),
                ui,
                draw_list: &draw_list,
                font_size: ui.current_font_size(),
            };
            // An invisible button for the canvas allows us to catch input for it.
            ui.invisible_button(im_str!("canvas"), content_region_avail.into());
            self.interact(&info);

            if self.options.canvas_width_ns <= 0.0 {
                self.options.canvas_width_ns = (max_ns - min_ns) as f32;
            }

            let options = &self.options;
            paint_timeline(&info, options, min_ns, max_ns);

            // We paint the threads bottom up
            let mut cursor_y = info.canvas_max.y;
            cursor_y -= info.font_size; // Leave room for time labels

            for (thread_info, stream) in &frame.thread_streams {
                // Visual separator between threads:
                info.draw_list
                    .add_line(
                        [info.canvas_min.x, cursor_y],
                        [info.canvas_max.x, cursor_y],
                        [1.0, 1.0, 1.0, 0.5],
                    )
                    .build();

                cursor_y -= info.font_size;
                let text_pos = [content_min.x, cursor_y];
                paint_thread_info(&info, thread_info, text_pos);
                info.canvas_max.y = cursor_y;

                let mut paint_stream = || -> Result<()> {
                    let top_scopes = Reader::from_start(stream).read_top_scopes()?;
                    if options.merge_scopes {
                        let merges = puffin::merge_top_scopes(&top_scopes);
                        for merge in merges {
                            paint_merge_scope(&info, options, stream, &merge, 0, &mut cursor_y)?;
                        }
                    } else {
                        for scope in top_scopes {
                            paint_scope(&info, options, stream, &scope, 0, &mut cursor_y)?;
                        }
                    }
                    Ok(())
                };
                if let Err(err) = paint_stream() {
                    let text = format!("Profiler stream error: {:?}", err);
                    info.draw_list
                        .add_text([info.canvas_min.x, cursor_y], ERROR_COLOR, &text);
                }

                cursor_y -= info.font_size; // Extra spacing between threads
            }
        });
    }

    /// Returns hovered, if any
    fn show_frames(&mut self, ui: &Ui<'_>) -> Option<Arc<FrameData>> {
        let frames = self.frames();

        let mut hovered_frame = None;

        let longest_count = frames.recent.len().max(frames.slowest.len());

        ui.text("Recent history:");
        self.show_frame_list(
            ui,
            "Recent frames",
            &frames.recent,
            longest_count,
            &mut hovered_frame,
        );
        ui.text("Slowest frames:");
        self.show_frame_list(
            ui,
            "Slow spikes",
            &frames.slowest,
            longest_count,
            &mut hovered_frame,
        );

        hovered_frame
    }

    fn show_frame_list(
        &mut self,
        ui: &Ui<'_>,
        label: &str,
        frames: &[Arc<FrameData>],
        longest_count: usize,
        hovered_frame: &mut Option<Arc<FrameData>>,
    ) {
        let mut slowest_frame = 0;
        for frame in frames {
            slowest_frame = frame.duration_ns().max(slowest_frame);
        }

        let min: Vec2 = ui.cursor_screen_pos().into();
        let size = Vec2::new(ui.content_region_avail()[0], 48.0);
        let max = min + size;

        let frame_width_including_spacing = (size.x / (longest_count as f32)).max(4.0).min(20.0);
        let frame_spacing = 2.0;
        let frame_width = frame_width_including_spacing - frame_spacing;

        ui.invisible_button(&ImString::new(label), size.into());
        let draw_list = ui.get_window_draw_list();

        let selected_frame_index = self.selected_frame_index();

        let mouse_pos: Vec2 = ui.io().mouse_pos.into();

        draw_list.with_clip_rect_intersect(min.into(), max.into(), || {
            for (i, frame) in frames.iter().enumerate() {
                let x = max.x - (frames.len() as f32 - i as f32) * frame_width_including_spacing;
                let mut rect_min = Vec2::new(x, min.y);
                let rect_max = Vec2::new(x + frame_width, max.y);

                let duration = frame.duration_ns();

                let is_selected = Some(frame.frame_index) == selected_frame_index;

                let is_hovered = rect_min.x - 0.5 * frame_spacing <= mouse_pos.x
                    && mouse_pos.x < rect_max.x + 0.5 * frame_spacing
                    && rect_min.y <= mouse_pos.y
                    && mouse_pos.y <= rect_max.y;

                if is_hovered {
                    *hovered_frame = Some(frame.clone());
                    ui.tooltip_text(im_str!("{:.1} ms", frame.duration_ns() as f64 * 1e-6));
                }
                if is_hovered && ui.is_mouse_clicked(MouseButton::Left) {
                    self.selected = Selected::Specific(frame.clone());
                }

                let mut color = if is_selected {
                    [1.0; 4]
                } else if is_hovered {
                    HOVER_COLOR
                } else {
                    [0.6, 0.6, 0.4, 1.0]
                };

                // Transparent, full height:
                color[3] = if is_selected || is_hovered {
                    0.75
                } else {
                    0.25
                };
                draw_list
                    .add_rect(rect_min.into(), rect_max.into(), color)
                    .filled(true)
                    .build();

                // Opaque, height based on duration:
                color[3] = 1.0;
                rect_min.y = lerp(max.y..=min.y, duration as f32 / slowest_frame as f32);
                draw_list
                    .add_rect(rect_min.into(), rect_max.into(), color)
                    .filled(true)
                    .build();
            }
        });
    }

    fn interact(&mut self, info: &Info<'_>) {
        let ui = &info.ui;

        let pan_button = MouseButton::Left;
        self.is_panning |= ui.is_item_hovered() && ui.is_mouse_clicked(pan_button);
        self.is_panning &= !ui.is_mouse_released(pan_button);

        let pan_delta = if self.is_panning {
            let pan_delta = ui.mouse_drag_delta(pan_button);
            ui.reset_mouse_drag_delta(pan_button);
            pan_delta
        } else {
            [0.0, 0.0]
        };

        if self.is_panning {
            self.options.sideways_pan_in_pixels += pan_delta[0];
        }

        if ui.is_item_hovered() {
            // note: imgui scroll coordinates are not pixels.
            // for `mouse_wheel` one unit scrolls "about 5 lines of text",
            // and `mouse_wheel_h` is unspecified.

            let zoom_factor = (ui.io().mouse_wheel * self.options.scroll_zoom_speed).exp();

            self.options.canvas_width_ns /= zoom_factor;
            let zoom_center = info.mouse_pos.x - info.canvas_min.x;
            self.options.sideways_pan_in_pixels =
                (self.options.sideways_pan_in_pixels - zoom_center) * zoom_factor + zoom_center;
        }
    }
}

fn paint_timeline(info: &Info<'_>, options: &Options, start_ns: NanoSecond, stop_ns: NanoSecond) {
    if options.canvas_width_ns <= 0.0 {
        return;
    }

    // We show all measurements relative to start_ns

    let max_lines = 300.0;
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
        let line_x = info.pixel_from_ns(options, start_ns + grid_ns);
        if line_x > info.canvas_max.x {
            break;
        }

        if info.canvas_min.x <= line_x {
            let big_line = grid_ns % (grid_spacing_ns * 100) == 0;
            let medium_line = grid_ns % (grid_spacing_ns * 10) == 0;

            let line_alpha = if big_line {
                big_alpha
            } else if medium_line {
                medium_alpha
            } else {
                tiny_alpha
            };
            let line_color = [1.0, 1.0, 1.0, line_alpha];

            info.draw_list
                .add_line(
                    [line_x, info.canvas_min.y],
                    [line_x, info.canvas_max.y],
                    line_color,
                )
                .build();

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
                let text_color = [1.0, 1.0, 1.0, (text_alpha * 2.0).min(1.0)];

                // Text at top:
                info.draw_list
                    .add_text([text_x, info.canvas_min.y], text_color, &text);

                // Text at bottom:
                info.draw_list.add_text(
                    [text_x, info.canvas_max.y - info.font_size],
                    text_color,
                    &text,
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
    info: &Info<'_>,
    options: &Options,
    prefix: &str,
    record: &Record<'_>,
    top_y: f32,
) -> PaintResult {
    let start_x = info.pixel_from_ns(options, record.start_ns);
    let stop_x = info.pixel_from_ns(options, record.stop_ns());
    let width = stop_x - start_x;
    if info.canvas_max.x < start_x || stop_x < info.canvas_min.x || width < options.cull_width {
        return PaintResult::Culled;
    }

    let bottom_y = top_y + options.rect_height;

    let is_hovered = start_x <= info.mouse_pos.x
        && info.mouse_pos.x <= stop_x
        && top_y <= info.mouse_pos.y
        && info.mouse_pos.y <= bottom_y;

    let rect_min = Vec2::new(start_x, top_y);
    let rect_max = Vec2::new(stop_x, bottom_y);
    let rect_color = if is_hovered {
        HOVER_COLOR
    } else {
        // options.rect_color
        color_from_duration(record.duration_ns)
    };
    let text_color = [0.0, 0.0, 0.0, 1.0];

    info.draw_list
        .add_rect(rect_min.into(), rect_max.into(), rect_color)
        .filled(true)
        .rounding(options.rounding)
        .build();

    let wide_enough_for_text = width > 32.0;
    if wide_enough_for_text {
        let rect_min = rect_min.max(info.canvas_min);
        let rect_max = rect_max.min(info.canvas_max);

        info.draw_list
            .with_clip_rect_intersect(rect_min.into(), rect_max.into(), || {
                let duration_ms = to_ms(record.duration_ns);
                let text = if record.data.is_empty() {
                    format!("{}{} {:6.3} ms", prefix, record.id, duration_ms)
                } else {
                    format!(
                        "{}{} {:?} {:6.3} ms",
                        prefix, record.id, record.data, duration_ms
                    )
                };
                info.draw_list.add_text(
                    [
                        start_x + 4.0,
                        top_y + 0.5 * (options.rect_height - info.font_size),
                    ],
                    text_color,
                    text,
                );
            });
    }

    if is_hovered {
        PaintResult::Hovered
    } else {
        PaintResult::Normal
    }
}

fn color_from_duration(ns: NanoSecond) -> [f32; 4] {
    let ms = to_ms(ns) as f32;
    // Brighter = more time.
    // So we start with dark colors (blue) and later bright colors (green).
    let b = remap_clamp(ms, 0.0..=5.0, 1.0..=0.3);
    let r = remap_clamp(ms, 0.0..=10.0, 0.5..=0.8);
    let g = remap_clamp(ms, 10.0..=20.0, 0.1..=0.8);
    let a = 0.9;
    [r, g, b, a]
}

fn to_ms(ns: NanoSecond) -> f64 {
    ns as f64 * 1e-6
}

use std::ops::{Add, Mul, RangeInclusive};

fn lerp<T>(range: RangeInclusive<T>, t: f32) -> T
where
    f32: Mul<T, Output = T>,
    T: Add<T, Output = T> + Copy,
{
    (1.0 - t) * *range.start() + t * *range.end()
}

fn remap_clamp(x: f32, from: RangeInclusive<f32>, to: RangeInclusive<f32>) -> f32 {
    let t = if x <= *from.start() {
        0.0
    } else if x >= *from.end() {
        1.0
    } else {
        (x - from.start()) / (from.end() - from.start())
    };
    lerp(to, t)
}

fn paint_scope(
    info: &Info<'_>,
    options: &Options,
    stream: &Stream,
    scope: &Scope<'_>,
    depth: usize,
    min_y: &mut f32,
) -> Result<PaintResult> {
    let top_y = info.canvas_max.y - (1.0 + depth as f32) * (options.rect_height + options.spacing);
    *min_y = min_y.min(top_y);

    let result = paint_record(info, options, "", &scope.record, top_y);

    if result != PaintResult::Culled {
        let mut num_children = 0;
        for child_scope in Reader::with_offset(stream, scope.child_begin_position)? {
            paint_scope(info, options, stream, &child_scope?, depth + 1, min_y)?;
            num_children += 1;
        }

        if result == PaintResult::Hovered {
            let ui = info.ui;
            ui.tooltip(|| {
                ui.text(&format!("id:       {}", scope.record.id));
                if !scope.record.location.is_empty() {
                    ui.text(&format!("location: {}", scope.record.location));
                }
                if !scope.record.data.is_empty() {
                    ui.text(&format!("data:     {}", scope.record.data));
                }
                ui.text(&format!(
                    "duration: {:6.3} ms",
                    to_ms(scope.record.duration_ns)
                ));
                ui.text(&format!("children: {}", num_children));
            });
        }
    }

    Ok(result)
}

fn paint_merge_scope(
    info: &Info<'_>,
    options: &Options,
    stream: &Stream,
    merge: &MergeScope<'_>,
    depth: usize,
    min_y: &mut f32,
) -> Result<PaintResult> {
    let top_y = info.canvas_max.y - (1.0 + depth as f32) * (options.rect_height + options.spacing);
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
            let ui = info.ui;
            ui.tooltip(|| merge_scope_tooltip(ui, merge));
        }
    }

    Ok(result)
}

fn merge_scope_tooltip(ui: &Ui<'_>, merge: &MergeScope<'_>) {
    ui.text(&format!("id:       {}", merge.record.id));
    if !merge.record.location.is_empty() {
        ui.text(&format!("location: {}", merge.record.location));
    }
    if !merge.record.data.is_empty() {
        ui.text(&format!("data:     {}", merge.record.data));
    }

    if merge.pieces.len() <= 1 {
        ui.text(&format!(
            "duration: {:6.3} ms",
            to_ms(merge.record.duration_ns)
        ));
    } else {
        ui.text(&format!("sum of:   {} scopes", merge.pieces.len()));
        ui.text(&format!(
            "total:    {:6.3} ms",
            to_ms(merge.record.duration_ns)
        ));

        ui.text(&format!(
            "mean:     {:6.3} ms",
            to_ms(merge.record.duration_ns) / (merge.pieces.len() as f64),
        ));
        let max_duration_ns = merge
            .pieces
            .iter()
            .map(|piece| piece.scope.record.duration_ns)
            .max()
            .unwrap();
        ui.text(&format!("max:      {:6.3} ms", to_ms(max_duration_ns)));
    }
}

fn paint_thread_info(info: &Info<'_>, thread_info: &ThreadInfo, pos: [f32; 2]) {
    let text = format!("{}", thread_info.name);
    let text_size = info.ui.calc_text_size(&ImString::new(&text), false, 0.0);

    info.draw_list
        .add_rect(
            pos,
            [pos[0] + text_size[0], pos[1] + text_size[1]],
            [0.0, 0.0, 0.0, 0.5],
        )
        .filled(true)
        .rounding(0.0)
        .build();

    info.draw_list.add_text(pos, [0.9, 0.9, 0.9, 1.0], &text);
}

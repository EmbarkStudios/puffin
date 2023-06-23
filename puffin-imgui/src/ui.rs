use imgui::*;
use mint::Vector2;
use puffin::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
const HOVER_COLOR: [f32; 4] = [0.8, 0.8, 0.8, 1.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum SortBy {
    Time,
    Name,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
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
    fn sort(
        self,
        thread_streams: &BTreeMap<ThreadInfo, Arc<StreamInfo>>,
    ) -> Vec<(ThreadInfo, Arc<StreamInfo>)> {
        let mut vec: Vec<_> = thread_streams
            .iter()
            .map(|(info, stream)| (info.clone(), stream.clone()))
            .collect();

        match self.sort_by {
            SortBy::Time => {
                vec.sort_by_key(|(info, _)| info.start_time_ns);
            }
            SortBy::Name => {
                vec.sort_by(|(a, _), (b, _)| natord::compare_ignore_case(&a.name, &b.name));
            }
        }
        if self.reversed {
            vec.reverse();
        }
        vec
    }

    fn ui(&mut self, ui: &imgui::Ui) {
        ui.text("Sort threads by:");
        ui.same_line();

        let dir = if self.reversed { '^' } else { 'v' };

        for &sort_by in &[SortBy::Time, SortBy::Name] {
            let selected = self.sort_by == sort_by;

            let label = if selected {
                format!("{sort_by:?} {dir}")
            } else {
                format!("{sort_by:?}")
            };

            if ui.radio_button_bool(label, selected) {
                if selected {
                    self.reversed = !self.reversed;
                } else {
                    self.sort_by = sort_by;
                    self.reversed = false;
                }
            }

            ui.same_line();
        }
        ui.new_line();
    }
}

#[derive(Clone, Debug, Default)]
struct Filter {
    filter: String,
}

impl Filter {
    fn ui(&mut self, ui: &imgui::Ui) {
        ui.text("Scope filter:");
        ui.same_line();
        ui.input_text("##scopefilter", &mut self.filter).build();
        self.filter = self.filter.to_lowercase();
        ui.same_line();
        if ui.button("X") {
            self.filter.clear();
        }
    }

    /// if true, show everything
    fn is_empty(&self) -> bool {
        self.filter.is_empty()
    }

    fn include(&self, id: &str) -> bool {
        if self.filter.is_empty() {
            true
        } else {
            id.to_lowercase().contains(&self.filter)
        }
    }
}

/// The frames we can select between
#[derive(Clone)]
pub struct Frames {
    pub recent: Vec<Arc<FrameData>>,
    pub slowest: Vec<Arc<FrameData>>,
}

impl Frames {
    fn all_uniq(&self) -> Vec<Arc<FrameData>> {
        let mut all = self.slowest.clone();
        all.extend(self.recent.iter().cloned());
        all.sort_by_key(|frame| frame.frame_index());
        all.dedup_by_key(|frame| frame.frame_index());
        all
    }
}

#[derive(Clone)]
pub struct Paused {
    /// The frame we are viewing.
    selected_frame: Arc<FrameData>,
    /// All the frames we had when paused.
    frames: Frames,
}

#[derive(Deserialize, Serialize)]
#[serde(default)]
pub struct ProfilerUi {
    pub options: Options,

    #[serde(skip)]
    frame_view: GlobalFrameView,

    // interaction:
    #[serde(skip)]
    is_panning: bool,
    #[serde(skip)]
    is_zooming: bool,

    /// If `None`, we show the latest frames.
    #[serde(skip)]
    paused: Option<Paused>,

    /// How we normalize the frame view:
    slowest_frame: f32,

    /// When did we last run a pass to pack all the frames?
    #[serde(skip)]
    last_pack_pass: Option<std::time::Instant>,
}

impl Default for ProfilerUi {
    fn default() -> Self {
        let frame_view = GlobalFrameView::default();
        frame_view.lock().set_max_recent(60 * 10); // We can't currently scroll back anyway

        Self {
            options: Default::default(),
            frame_view,
            is_panning: false,
            is_zooming: false,
            paused: None,
            slowest_frame: 0.17,
            last_pack_pass: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Options {
    // --------------------
    // View:
    /// Controls zoom
    pub canvas_width_ns: f32,

    /// How much we have panned sideways:
    pub sideways_pan_in_pixels: f32,

    // --------------------
    // Visuals:
    /// Events shorter than this many pixels aren't painted
    pub cull_width: f32,
    /// Draw each item with at least this width (only makes sense if [`Self::cull_width`] is 0)
    pub min_width: f32,

    pub rect_height: f32,
    pub spacing: f32,
    pub rounding: f32,

    /// Aggregate child scopes with the same id?
    pub merge_scopes: bool,

    pub sorting: Sorting,
    #[serde(skip)]
    filter: Filter,

    /// Size of a frame in the frame-view, including padding
    pub frame_width: f32,

    /// Set when user clicks a scope.
    /// First part is `now()`, second is range.
    #[serde(skip)]
    zoom_to_relative_ns_range: Option<(f64, (NanoSecond, NanoSecond))>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            canvas_width_ns: 0.0,
            sideways_pan_in_pixels: 0.0,

            // cull_width: 0.5, // save some CPU?
            cull_width: 0.0, // no culling
            min_width: 1.0,

            rect_height: 16.0,
            spacing: 4.0,
            rounding: 4.0,

            merge_scopes: true,

            sorting: Default::default(),
            filter: Default::default(),

            frame_width: 10.0,

            zoom_to_relative_ns_range: None,
        }
    }
}

/// Context for painting a frame.
struct Info<'a> {
    // Bounding box of canvas in pixels:
    canvas_min: Vec2,
    canvas_max: Vec2,

    mouse_pos: Vec2,

    ui: &'a Ui,
    draw_list: &'a DrawListMut<'a>,
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
    /// The frames we are looking at.
    pub fn global_frame_view(&self) -> &GlobalFrameView {
        &self.frame_view
    }

    /// Show a [`imgui::Window`] with the profiler contents.
    /// If you want to control the window yourself, use [`Self::ui`] instead.
    pub fn window(&mut self, ui: &Ui) -> bool {
        let mut open = true;
        ui.window("Profiler")
            .position([10.0, 25.0], Condition::FirstUseEver)
            .size([800.0, 600.0], Condition::FirstUseEver)
            .bg_alpha(0.99) // Transparency can be distracting
            .always_auto_resize(false)
            .opened(&mut open)
            .build(|| self.ui(ui));
        open
    }

    fn latest_frames(&self) -> Frames {
        let view = self.frame_view.lock();
        Frames {
            recent: view.recent_frames().cloned().collect(),
            slowest: view.slowest_frames_chronological().cloned().collect(),
        }
    }

    /// The frames we can select between
    fn frames(&self) -> Frames {
        self.paused
            .as_ref()
            .map_or_else(|| self.latest_frames(), |paused| paused.frames.clone())
    }

    /// Pause on the specific frame
    fn pause_and_select(&mut self, selected_frame: Arc<FrameData>) {
        if let Some(paused) = &mut self.paused {
            paused.selected_frame = selected_frame;
        } else {
            self.paused = Some(Paused {
                selected_frame,
                frames: self.frames(),
            });
        }
    }

    fn selected_frame(&self) -> Option<Arc<FrameData>> {
        self.paused
            .as_ref()
            .map(|paused| paused.selected_frame.clone())
            .or_else(|| self.frame_view.lock().latest_frame())
    }

    fn selected_frame_index(&self) -> Option<FrameIndex> {
        self.selected_frame().map(|frame| frame.frame_index())
    }

    fn all_known_frames(&self) -> Vec<Arc<FrameData>> {
        let mut all = self
            .frame_view
            .lock()
            .all_uniq()
            .cloned()
            .collect::<Vec<_>>();

        if let Some(paused) = &self.paused {
            all.append(&mut paused.frames.all_uniq());
        }

        all.sort_by_key(|frame| frame.frame_index());
        all.dedup_by_key(|frame| frame.frame_index());
        all
    }

    fn run_pack_pass_if_needed(&mut self) {
        if !self.frame_view.lock().pack_frames() {
            return;
        }
        let last_pack_pass = self
            .last_pack_pass
            .get_or_insert_with(std::time::Instant::now);
        let time_since_last_pack = last_pack_pass.elapsed();
        if time_since_last_pack > std::time::Duration::from_secs(1) {
            puffin::profile_scope!("pack_pass");
            for frame in self.all_known_frames() {
                if Some(frame.frame_index()) != self.selected_frame_index() {
                    frame.pack();
                }
            }
            self.last_pack_pass = Some(std::time::Instant::now());
        }
    }

    /// Show the profiler.
    ///
    /// Call this from within an [`imgui::Window`], or use [`Self::window`] instead.
    pub fn ui(&mut self, ui: &Ui) {
        #![allow(clippy::collapsible_else_if)]

        puffin::profile_function!();

        self.run_pack_pass_if_needed();

        let mut scopes_on = puffin::are_scopes_on();
        ui.checkbox("Profiling enabled", &mut scopes_on);
        puffin::set_scopes_on(scopes_on);

        if !puffin::are_scopes_on() {
            ui.same_line();
            ui.text_colored(ERROR_COLOR, "No new scopes are being recorded!");
        }

        let mut hovered_frame = None;
        if imgui::CollapsingHeader::new("Frames")
            .default_open(false)
            .build(ui)
        {
            ui.indent();
            hovered_frame = self.show_frames(ui);
            ui.unindent();
        }

        let frame = hovered_frame.or_else(|| self.selected_frame());
        let frame = if let Some(frame) = frame {
            frame
        } else {
            ui.text("No profiling data");
            return;
        };

        let frame = match frame.unpacked() {
            Ok(frame) => frame,
            Err(err) => {
                ui.text_colored(ERROR_COLOR, format!("Bad frame: {err}"));
                return;
            }
        };

        // TODO: show age of data

        let (min_ns, max_ns) = frame.range_ns();

        ui.button("Help!");
        if ui.is_item_hovered() {
            ui.tooltip_text(
                "Drag to pan. \n\
                Zoom: drag up/down with secondary mouse button. \n\
                Click on a scope to zoom to it.\n\
                Double-click background to reset view.\n\
                Press spacebar to pause/resume.",
            );
        }

        ui.same_line();

        let play_pause_button_size = [54.0, 0.0];
        if self.paused.is_some() {
            if ui.button_with_size("Resume", play_pause_button_size)
                || ui.is_key_pressed(imgui::Key::Space)
            {
                self.paused = None;
            }
        } else {
            if ui.button_with_size("Pause", play_pause_button_size)
                || ui.is_key_pressed(imgui::Key::Space)
            {
                let latest = self.frame_view.lock().latest_frame();
                if let Some(latest) = latest {
                    self.pause_and_select(latest);
                }
            }
        }
        if ui.is_item_hovered() {
            ui.tooltip_text("Toggle with spacebar.");
        }

        ui.same_line();
        ui.checkbox(
            "Merge children with same ID",
            &mut self.options.merge_scopes,
        );

        ui.text(format!(
            "Showing frame #{}, {:.1} ms, {} threads, {} scopes.",
            frame.frame_index(),
            (max_ns - min_ns) as f64 * 1e-6,
            frame.thread_streams.len(),
            frame.meta.num_scopes,
        ));

        // The number of threads can change between frames, so always show this even if there currently is only one thread:
        self.options.sorting.ui(ui);

        self.options.filter.ui(ui);

        ui.separator();

        let content_min: Vec2 = ui.cursor_screen_pos().into();
        let content_region_avail: Vec2 = ui.content_region_avail().into();
        let content_max = content_min + content_region_avail;

        let draw_list = ui.get_window_draw_list();

        // Make it scrollable:
        ui.child_window("flamegraph").build(|| {
            let info = Info {
                start_ns: min_ns,
                canvas_min: content_min,
                canvas_max: content_max,
                mouse_pos: ui.io().mouse_pos.into(),
                ui,
                draw_list: &draw_list,
                font_size: ui.current_font_size(),
            };

            draw_list.with_clip_rect_intersect(
                Vector2::from_slice(&[content_min.x, content_min.y]),
                Vector2::from_slice(&[content_max.x, content_max.y]),
                || {
                    let max_y = self.ui_canvas(&info, &frame, (min_ns, max_ns));
                    let used_space = Vec2::new(
                        content_region_avail.x,
                        content_region_avail.y.max(max_y - content_min.y),
                    );

                    // An invisible button for the canvas allows us to catch input for it.
                    ui.invisible_button(
                        "canvas",
                        Vector2::from_slice(&[used_space.x, used_space.y]),
                    );
                    self.interact_with_canvas(ui, max_ns - min_ns, (content_min, content_max));
                },
            );
        });
    }

    fn ui_canvas(
        &mut self,
        info: &Info<'_>,
        frame: &Arc<UnpackedFrameData>,
        (min_ns, max_ns): (NanoSecond, NanoSecond),
    ) -> f32 {
        puffin::profile_function!();

        if self.options.canvas_width_ns <= 0.0 {
            self.options.canvas_width_ns = (max_ns - min_ns) as f32;
            self.options.zoom_to_relative_ns_range = None;
        }

        paint_timeline(info, &self.options, min_ns);

        // We paint the threads top-down
        let mut cursor_y = info.canvas_min.y - info.ui.scroll_y();
        cursor_y += info.font_size; // Leave room for time labels

        let thread_streams = self.options.sorting.sort(&frame.thread_streams);

        for (thread_info, stream_info) in &thread_streams {
            cursor_y += 2.0;
            let line_y = cursor_y;
            cursor_y += 2.0;

            let text_pos = [info.canvas_min.x, cursor_y];
            paint_thread_info(info, thread_info, text_pos);
            cursor_y += info.font_size;

            // Visual separator between threads:
            info.draw_list
                .add_line(
                    [info.canvas_min.x, line_y],
                    [info.canvas_max.x, line_y],
                    [1.0, 1.0, 1.0, 0.5],
                )
                .build();

            let mut paint_stream = || -> Result<()> {
                if self.options.merge_scopes {
                    let frames = vec![frame.clone()];
                    let merges = {
                        puffin::profile_scope!("merge_scopes");
                        puffin::merge_scopes_for_thread(&frames, thread_info)?
                    };
                    for merge in merges {
                        paint_merge_scope(info, &mut self.options, 0, &merge, 0, cursor_y)?;
                    }
                } else {
                    let top_scopes = Reader::from_start(&stream_info.stream).read_top_scopes()?;
                    for scope in top_scopes {
                        paint_scope(
                            info,
                            &mut self.options,
                            &stream_info.stream,
                            &scope,
                            0,
                            cursor_y,
                        )?;
                    }
                }
                Ok(())
            };
            if let Err(err) = paint_stream() {
                let text = format!("Profiler stream error: {err:?}");
                info.draw_list
                    .add_text([info.canvas_min.x, cursor_y], ERROR_COLOR, text);
            }

            cursor_y +=
                stream_info.depth as f32 * (self.options.rect_height + self.options.spacing);

            cursor_y += info.font_size; // Extra spacing between threads
        }

        cursor_y + info.ui.scroll_y()
    }

    /// Returns hovered, if any
    fn show_frames(&mut self, ui: &Ui) -> Option<Arc<FrameData>> {
        puffin::profile_function!();
        let frames = self.frames();

        let mut hovered_frame = None;

        max_memory_controls(ui, &frames, &self.frame_view);

        ui.columns(2, "columns", false);
        ui.set_column_width(0, 64.0);

        ui.text("Recent:");
        ui.next_column();

        let slowest_visible = self.show_frame_list(
            ui,
            "Recent frames",
            &frames.recent,
            &mut hovered_frame,
            self.slowest_frame,
        );
        // quickly, but smoothly, normalize frame height:
        self.slowest_frame = lerp(self.slowest_frame..=slowest_visible as f32, 0.2);
        ui.next_column();

        ui.text("Slowest:");
        if ui.button("Clear") {
            self.frame_view.lock().clear_slowest();
        }
        ui.next_column();
        {
            let num_fit = (ui.content_region_avail()[0] / self.options.frame_width).floor();
            let num_fit = (num_fit as usize).clamp(1, frames.slowest.len());
            let slowest_of_the_slow = puffin::select_slowest(&frames.slowest, num_fit);

            let mut slowest_frame = 0;
            for frame in &slowest_of_the_slow {
                slowest_frame = frame.duration_ns().max(slowest_frame);
            }

            self.show_frame_list(
                ui,
                "Slow spikes",
                &slowest_of_the_slow,
                &mut hovered_frame,
                slowest_frame as f32,
            );
        }
        ui.next_column();

        ui.columns(1, "", false);

        hovered_frame
    }

    /// Returns the slowest visible frame
    fn show_frame_list(
        &mut self,
        ui: &Ui,
        label: &str,
        frames: &[Arc<FrameData>],
        hovered_frame: &mut Option<Arc<FrameData>>,
        slowest_frame: f32,
    ) -> NanoSecond {
        let min: Vec2 = ui.cursor_screen_pos().into();
        let size = Vec2::new(ui.content_region_avail()[0], 48.0);
        let max = min + size;

        let frame_width_including_spacing = self.options.frame_width;
        let frame_spacing = 2.0;
        let frame_width = frame_width_including_spacing - frame_spacing;

        ui.invisible_button(ImString::new(label), Vector2::from_slice(&[size.x, size.y]));
        let draw_list = ui.get_window_draw_list();

        let selected_frame_index = self.selected_frame_index();

        let mouse_pos: Vec2 = ui.io().mouse_pos.into();

        let mut slowest_visible_frame = 0;

        draw_list.with_clip_rect_intersect(
            Vector2::from_slice(&[min.x, min.y]),
            Vector2::from_slice(&[max.x, max.y]),
            || {
                for (i, frame) in frames.iter().enumerate() {
                    let x =
                        max.x - (frames.len() as f32 - i as f32) * frame_width_including_spacing;
                    let mut rect_min = Vec2::new(x, min.y);
                    let rect_max = Vec2::new(x + frame_width, max.y);

                    let is_visible = min.x <= rect_max.x && rect_min.x <= max.x;
                    if is_visible {
                        let duration = frame.duration_ns();
                        slowest_visible_frame = duration.max(slowest_visible_frame);

                        let is_selected = Some(frame.frame_index()) == selected_frame_index;

                        let is_hovered = rect_min.x - 0.5 * frame_spacing <= mouse_pos.x
                            && mouse_pos.x < rect_max.x + 0.5 * frame_spacing
                            && rect_min.y <= mouse_pos.y
                            && mouse_pos.y <= rect_max.y;

                        if is_hovered {
                            *hovered_frame = Some(frame.clone());
                            ui.tooltip_text(format!("{:.1} ms", frame.duration_ns() as f64 * 1e-6));
                        }
                        if is_hovered && ui.is_mouse_clicked(MouseButton::Left) {
                            self.pause_and_select(frame.clone());
                        }

                        let mut color = if is_selected {
                            [1.0; 4]
                        } else if is_hovered {
                            HOVER_COLOR
                        } else {
                            [0.6, 0.6, 0.4, 1.0]
                        };

                        // Transparent, full height:
                        color[3] = if is_selected || is_hovered { 0.6 } else { 0.25 };
                        draw_list
                            .add_rect(
                                Vector2::from_slice(&[rect_min.x, rect_min.y]),
                                Vector2::from_slice(&[rect_max.x, rect_max.y]),
                                color,
                            )
                            .filled(true)
                            .build();

                        // Opaque, height based on duration:
                        color[3] = 1.0;
                        rect_min.y = lerp(max.y..=min.y, duration as f32 / slowest_frame);
                        draw_list
                            .add_rect(
                                Vector2::from_slice(&[rect_min.x, rect_min.y]),
                                Vector2::from_slice(&[rect_max.x, rect_max.y]),
                                color,
                            )
                            .filled(true)
                            .build();
                    }
                }
            },
        );

        slowest_visible_frame
    }

    fn interact_with_canvas(
        &mut self,
        ui: &Ui,
        duration_ns: NanoSecond,
        (canvas_min, canvas_max): (Vec2, Vec2),
    ) {
        // note: imgui scroll coordinates are not pixels.
        // for `mouse_wheel` one unit scrolls "about 5 lines of text",
        // and `mouse_wheel_h` is unspecified.
        // So let's not use them.

        let pan_button = MouseButton::Left;
        self.is_panning |= ui.is_item_hovered() && ui.is_mouse_clicked(pan_button);
        self.is_panning &= !ui.is_mouse_released(pan_button);

        let zoom_button = MouseButton::Right;
        self.is_zooming |= ui.is_item_hovered() && ui.is_mouse_clicked(zoom_button);
        self.is_zooming &= !ui.is_mouse_released(zoom_button);

        let pan_delta = if self.is_panning {
            let pan_delta = ui.mouse_drag_delta_with_button(pan_button);
            ui.reset_mouse_drag_delta(pan_button);
            pan_delta
        } else {
            [0.0, 0.0]
        };

        if self.is_panning && pan_delta[0] != 0.0 {
            self.options.sideways_pan_in_pixels += pan_delta[0];
            self.options.zoom_to_relative_ns_range = None;
        }

        let zoom_factor = if self.is_zooming {
            let zoom_delta = ui.mouse_drag_delta_with_button(zoom_button)[1];
            ui.reset_mouse_drag_delta(zoom_button);
            (zoom_delta * 0.01).exp()
        } else {
            0.0
        };

        if zoom_factor != 0.0 {
            self.options.canvas_width_ns /= zoom_factor;

            let zoom_center = ui.io().mouse_pos[0] - canvas_min.x;
            self.options.sideways_pan_in_pixels =
                (self.options.sideways_pan_in_pixels - zoom_center) * zoom_factor + zoom_center;

            self.options.zoom_to_relative_ns_range = None;
        }

        if ui.is_item_hovered() && ui.is_mouse_double_clicked(MouseButton::Left) {
            // Reset view
            self.options.zoom_to_relative_ns_range = Some((now(), (0, duration_ns)));
        }

        if let Some((start_time, (start_ns, end_ns))) = self.options.zoom_to_relative_ns_range {
            const ZOOM_DURATION: f32 = 0.75;
            let t = ((now() - start_time) as f32 / ZOOM_DURATION).min(1.0);

            let canvas_width = canvas_max.x - canvas_min.x;

            let target_canvas_width_ns = (end_ns - start_ns) as f32;
            let target_pan_in_pixels = -canvas_width * start_ns as f32 / target_canvas_width_ns;

            // self.options.canvas_width_ns =
            //     lerp(self.options.canvas_width_ns..=target_canvas_width_ns, t);
            self.options.canvas_width_ns = lerp(
                self.options.canvas_width_ns.recip()..=target_canvas_width_ns.recip(),
                t,
            )
            .recip();
            self.options.sideways_pan_in_pixels = lerp(
                self.options.sideways_pan_in_pixels..=target_pan_in_pixels,
                t,
            );

            if t >= 1.0 {
                self.options.zoom_to_relative_ns_range = None;
            }
        }
    }
}

/// Current time in seconds
fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

fn paint_timeline(info: &Info<'_>, options: &Options, start_ns: NanoSecond) {
    if options.canvas_width_ns <= 0.0 {
        return;
    }

    let alpha_multiplier = if options.filter.is_empty() { 1.0 } else { 0.3 };

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
            let line_color = [1.0, 1.0, 1.0, line_alpha * alpha_multiplier];

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
        format!("{grid_ms:.0} ms")
    } else if grid_ns % 100_000 == 0 {
        format!("{grid_ms:.1} ms")
    } else if grid_ns % 10_000 == 0 {
        format!("{grid_ms:.2} ms")
    } else {
        format!("{grid_ms:.3} ms")
    }
}

fn paint_record(
    info: &Info<'_>,
    options: &mut Options,
    prefix: &str,
    record: &Record<'_>,
    top_y: f32,
) -> PaintResult {
    let mut start_x = info.pixel_from_ns(options, record.start_ns);
    let mut stop_x = info.pixel_from_ns(options, record.stop_ns());
    if info.canvas_max.x < start_x
        || stop_x < info.canvas_min.x
        || stop_x - start_x < options.cull_width
    {
        return PaintResult::Culled;
    }

    let mut min_width = options.min_width;

    let bottom_y = top_y + options.rect_height;

    let is_hovered = start_x <= info.mouse_pos.x
        && info.mouse_pos.x <= stop_x
        && top_y <= info.mouse_pos.y
        && info.mouse_pos.y <= bottom_y;

    if is_hovered && info.ui.is_mouse_clicked(MouseButton::Left) {
        options.zoom_to_relative_ns_range = Some((
            now(),
            (
                record.start_ns - info.start_ns,
                record.stop_ns() - info.start_ns,
            ),
        ));
    }

    let mut rect_color = if is_hovered {
        HOVER_COLOR
    } else {
        color_from_duration(record.duration_ns)
    };

    if !options.filter.is_empty() {
        if options.filter.include(record.id) {
            // keep full opacity
            min_width *= 2.0; // make it more visible even when thin
        } else {
            rect_color[3] *= 0.3; // fade to highlight others
        }
    }

    if stop_x - start_x < min_width {
        // Make sure it is visible:
        let center = 0.5 * (start_x + stop_x);
        start_x = center - 0.5 * min_width;
        stop_x = center + 0.5 * min_width;
    }

    let rect_min = Vec2::new(start_x, top_y);
    let rect_max = Vec2::new(stop_x, bottom_y);

    let text_color = [0.0, 0.0, 0.0, 1.0];

    info.draw_list
        .add_rect(
            Vector2::from_slice(&[rect_min.x, rect_min.y]),
            Vector2::from_slice(&[rect_max.x, rect_max.y]),
            rect_color,
        )
        .filled(true)
        .rounding(options.rounding)
        .build();

    let wide_enough_for_text = stop_x - start_x > 32.0;
    if wide_enough_for_text {
        let rect_min = rect_min.max(info.canvas_min);
        let rect_max = rect_max.min(info.canvas_max);

        info.draw_list.with_clip_rect_intersect(
            Vector2::from_slice(&[rect_min.x, rect_min.y]),
            Vector2::from_slice(&[rect_max.x, rect_max.y]),
            || {
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
            },
        );
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
    let g = remap_clamp(ms, 10.0..=33.3, 0.1..=0.8);
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
            let ui = info.ui;
            ui.tooltip(|| {
                ui.text(format!("id:       {}", scope.record.id));
                if !scope.record.location.is_empty() {
                    ui.text(format!("location: {}", scope.record.location));
                }
                if !scope.record.data.is_empty() {
                    ui.text(format!("data:     {}", scope.record.data));
                }
                ui.text(format!(
                    "duration: {:6.3} ms",
                    to_ms(scope.record.duration_ns)
                ));
                ui.text(format!("children: {num_children}"));
            });
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
) -> Result<PaintResult> {
    let top_y = min_y + (depth as f32) * (options.rect_height + options.spacing);

    let prefix = if merge.num_pieces <= 1 {
        String::default()
    } else {
        format!("{}x ", merge.num_pieces)
    };

    let record = Record {
        start_ns: ns_offset + merge.relative_start_ns,
        duration_ns: merge.duration_per_frame_ns,
        id: &merge.id,
        location: &merge.location,
        data: &merge.data,
    };

    let result = paint_record(info, options, &prefix, &record, top_y);

    if result != PaintResult::Culled {
        for merged_child in &merge.children {
            paint_merge_scope(
                info,
                options,
                record.start_ns,
                merged_child,
                depth + 1,
                min_y,
            )?;
        }

        if result == PaintResult::Hovered {
            let ui = info.ui;
            ui.tooltip(|| merge_scope_tooltip(ui, merge));
        }
    }

    Ok(result)
}

fn merge_scope_tooltip(ui: &Ui, merge: &MergeScope<'_>) {
    ui.text(format!("id:       {}", merge.id));
    if !merge.location.is_empty() {
        ui.text(format!("location: {}", merge.location));
    }
    if !merge.data.is_empty() {
        ui.text(format!("data:     {}", merge.data));
    }

    if merge.num_pieces <= 1 {
        ui.text(format!(
            "duration: {:6.3} ms",
            to_ms(merge.duration_per_frame_ns)
        ));
    } else {
        ui.text(format!("sum of:   {} scopes", merge.num_pieces));
        ui.text(format!(
            "total:    {:6.3} ms",
            to_ms(merge.duration_per_frame_ns)
        ));
        ui.text(format!(
            "mean:     {:6.3} ms",
            to_ms(merge.duration_per_frame_ns) / (merge.num_pieces as f64),
        ));
        ui.text(format!("max:      {:6.3} ms", to_ms(merge.max_duration_ns)));
    }
}

fn paint_thread_info(info: &Info<'_>, thread_info: &ThreadInfo, pos: [f32; 2]) {
    let text = &thread_info.name;
    let text_size = info.ui.calc_text_size(ImString::new(text));

    info.draw_list
        .add_rect(
            pos,
            [pos[0] + text_size[0], pos[1] + text_size[1]],
            [0.0, 0.0, 0.0, 0.5],
        )
        .filled(true)
        .rounding(0.0)
        .build();

    info.draw_list.add_text(pos, [0.9, 0.9, 0.9, 1.0], text);
}

fn max_memory_controls(ui: &Ui, frames: &Frames, frame_view: &GlobalFrameView) {
    let uniq = frames.all_uniq();

    let mut bytes = 0;
    let mut unpacked = 0;
    for frame in &uniq {
        bytes += frame.bytes_of_ram_used();
        unpacked += frame.has_unpacked() as usize;
    }
    ui.text(format!(
        "{} frames ({} unpacked) using approximately {:.1} MB.",
        uniq.len(),
        unpacked,
        bytes as f64 * 1e-6
    ));

    let frames_per_second = if let (Some(first), Some(last)) = (uniq.first(), uniq.last()) {
        let nanos = last.range_ns().1 - first.range_ns().0;
        let seconds = nanos as f64 * 1e-9;
        let frames = last.frame_index() - first.frame_index() + 1;
        frames as f64 / seconds
    } else {
        60.0
    };

    let mut memory_length = frame_view.lock().max_recent() as u64;

    ui.slider_config("Max recent frames to store", 10, 100_000)
        .display_format(format!(
            "%d (~ {:.1} minutes, ~ {:.0} MB)",
            memory_length as f64 / 60.0 / frames_per_second,
            memory_length as f64 * bytes as f64 / uniq.len() as f64 * 1e-6,
        ))
        .build(&mut memory_length);

    frame_view.lock().set_max_recent(memory_length as _);
}

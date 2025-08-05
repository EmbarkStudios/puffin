//! Remote puffin viewer, connecting to a [`puffin_http::Server`].

#![forbid(unsafe_code)]
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
                ui.label(format!("Viewing {name}"));
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
    pub fn new(source: Source, storage: Option<&dyn eframe::Storage>) -> Self {
        let profiler_ui = storage
            .and_then(|storage| eframe::get_value(storage, eframe::APP_KEY))
            .unwrap_or_default();

        Self {
            profiler_ui,
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
            let mut file = match std::fs::File::create(path) {
                Ok(file) => file,
                Err(error) => {
                    self.error = Some(format!("Failed to create file: {error:#}"));
                    return;
                }
            };

            if let Err(error) = self.source.frame_view().write(&mut file) {
                self.error = Some(format!("Failed to export: {error:#}"));
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

        let mut file = match std::fs::File::open(&path) {
            Ok(bytes) => bytes,
            Err(err) => {
                self.error = Some(format!("Failed to open {}: {err:#}", path.display()));
                return;
            }
        };

        match FrameView::read(&mut file) {
            Ok(frame_view) => {
                self.profiler_ui.reset();
                self.source = Source::FilePath(path, frame_view);
                self.error = None;
            }
            Err(err) => {
                self.error = Some(format!("Failed to load {}: {err:#}", path.display()));
            }
        }
    }

    fn open_puffin_bytes(&mut self, name: String, bytes: &[u8]) {
        puffin::profile_function!();
        let mut reader = std::io::Cursor::new(bytes);
        match FrameView::read(&mut reader) {
            Ok(frame_view) => {
                self.profiler_ui.reset();
                self.source = Source::FileName(name, frame_view);
                self.error = None;
            }
            Err(err) => {
                self.error = Some(format!("Failed to load file {name:?}: {err}"));
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn ui_menu_bar(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::O)) {
            self.open_dialog();
        }

        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S)) {
            self.save_dialog();
        }

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                egui::widgets::global_theme_preference_switch(ui);

                ui.menu_button("File", |ui| {
                    if ui.button("Open…").clicked() {
                        self.open_dialog();
                    }

                    if ui.button("Save as…").clicked() {
                        self.save_dialog();
                    }

                    if ui.button("Quit").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
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
        if !ctx.input(|i| i.raw.hovered_files.is_empty()) {
            let painter =
                ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("file_drop_target")));

            let screen_rect = ctx.input(|i| i.screen_rect());
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
        ctx.input(|i| {
            if !i.raw.dropped_files.is_empty() {
                for file in i.raw.dropped_files.iter() {
                    if let Some(path) = &file.path {
                        self.open_puffin_path(path.clone());
                        break;
                    } else if let Some(bytes) = &file.bytes {
                        self.open_puffin_bytes(file.name.clone(), bytes);
                        break;
                    }
                }
            }
        });
    }
}

impl eframe::App for PuffinViewer {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.profiler_ui);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        puffin::GlobalProfiler::lock().new_frame();

        #[cfg(not(target_arch = "wasm32"))]
        {
            self.ui_menu_bar(ctx);
        }

        #[cfg(target_arch = "wasm32")]
        {
            egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
                ui.heading("Puffin Viewer, on the web");
                ui.horizontal_wrapped(|ui| {
                    ui.label("It is recommended that you instead use the native version: ");
                    ui.code("cargo install puffin_viewer --locked");
                });
                ui.hyperlink("https://github.com/EmbarkStudios/puffin");
            });
        }

        egui::TopBottomPanel::bottom("info_bar").show(ctx, |ui| {
            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::RED, error);
                ui.add_space(4.0);
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
mod web;

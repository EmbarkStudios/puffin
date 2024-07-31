use egui::TextBuffer;
use puffin::*;

use crate::{add_space, filter::Filter};

#[derive(Clone, Debug, Default)]
pub struct Options {
    filter: Filter,
}

pub fn ui(
    ui: &mut egui::Ui,
    options: &mut Options,
    scope_infos: &ScopeCollection,
    frames: &[std::sync::Arc<UnpackedFrameData>],
) {
    let mut threads = std::collections::HashSet::<&ThreadInfo>::new();
    let mut stats = Stats::default();

    for frame in frames {
        threads.extend(frame.thread_streams.keys());
        for (thread_info, stream) in &frame.thread_streams {
            collect_stream(&mut stats, &thread_info.name, &stream.stream).ok();
        }
    }

    let mut total_bytes = 0;
    let mut total_ns = 0;
    for scope in stats.scopes.values() {
        total_bytes += scope.bytes;
        total_ns += scope.total_self_ns;
    }

    ui.label("This view can be used to find scopes that use up a lot of bandwidth, and should maybe be removed.");

    ui.label(format!(
        "Current frame: {} unique scopes, using a total of {:.1} kB, covering {:.1} ms over {} thread(s)",
        stats.scopes.len(),
        total_bytes as f32 * 1e-3,
        total_ns as f32 * 1e-6,
        threads.len()
    ));

    add_space(ui);

    options.filter.ui(ui);

    add_space(ui);

    let mut scopes: Vec<_> = stats
        .scopes
        .iter()
        .map(|(key, value)| (key, *value))
        .collect();
    scopes.sort_by_key(|(key, _)| *key);
    scopes.sort_by_key(|(_key, scope_stats)| scope_stats.bytes);
    scopes.reverse();

    egui::ScrollArea::horizontal().show(ui, |ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

        egui_extras::TableBuilder::new(ui)
            .columns(
                egui_extras::Column::auto_with_initial_suggestion(200.0).resizable(true),
                9,
            )
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.heading("Thread");
                });
                header.col(|ui| {
                    ui.heading("Location");
                });
                header.col(|ui| {
                    ui.heading("Function Name");
                });
                header.col(|ui| {
                    ui.heading("Scope Name");
                });
                header.col(|ui| {
                    ui.heading("Count");
                });
                header.col(|ui| {
                    ui.heading("Size");
                });
                header.col(|ui| {
                    ui.heading("Total self time");
                });
                header.col(|ui| {
                    ui.heading("Mean self time");
                });
                header.col(|ui| {
                    ui.heading("Max self time");
                });
            })
            .body(|mut body| {
                for (key, stats) in &scopes {
                    let Some(scope_details) = scope_infos.fetch_by_id(&key.id) else {
                        continue;
                    };

                    if !options.filter.include(scope_details.name()) {
                        return;
                    }

                    body.row(14.0, |mut row| {
                        row.col(|ui| {
                            ui.label(&key.thread_name);
                        });
                        row.col(|ui| {
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                            ui.label(scope_details.location());
                        });
                        row.col(|ui| {
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                            ui.label(scope_details.function_name.as_str());
                        });

                        row.col(|ui| {
                            if let Some(name) = &scope_details.scope_name {
                                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                                ui.label(name.as_ref());
                            }
                        });
                        row.col(|ui| {
                            ui.monospace(format!("{:>5}", stats.count));
                        });
                        row.col(|ui| {
                            ui.monospace(format!("{:>6.1} kB", stats.bytes as f32 * 1e-3));
                        });
                        row.col(|ui| {
                            ui.monospace(format!("{:>8.1} µs", stats.total_self_ns as f32 * 1e-3));
                        });
                        row.col(|ui| {
                            ui.monospace(format!(
                                "{:>8.1} µs",
                                stats.total_self_ns as f32 * 1e-3 / (stats.count as f32)
                            ));
                        });
                        row.col(|ui| {
                            ui.monospace(format!("{:>8.1} µs", stats.max_ns as f32 * 1e-3));
                        });
                    });
                }
            });
    });
}

#[derive(Default)]
struct Stats {
    scopes: std::collections::HashMap<Key, ScopeStats>,
}

#[derive(Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    id: ScopeId,
    thread_name: String,
}

#[derive(Copy, Clone, Default)]
struct ScopeStats {
    count: usize,
    bytes: usize,
    /// Time covered by all scopes, minus those covered by child scopes.
    /// A lot of time == useful scope.
    total_self_ns: NanoSecond,
    /// Time covered by the slowest scope, minus those covered by child scopes.
    /// A lot of time == useful scope.
    max_ns: NanoSecond,
}

fn collect_stream(
    stats: &mut Stats,
    thread_name: &str,
    stream: &puffin::Stream,
) -> puffin::Result<()> {
    for scope in puffin::Reader::from_start(stream) {
        collect_scope(stats, thread_name, stream, &scope?)?;
    }
    Ok(())
}

fn collect_scope<'s>(
    stats: &mut Stats,
    thread_name: &str,
    stream: &'s puffin::Stream,
    scope: &puffin::Scope<'s>,
) -> puffin::Result<()> {
    let mut ns_used_by_children = 0;
    for child_scope in Reader::with_offset(stream, scope.child_begin_position)? {
        let child_scope = &child_scope?;
        collect_scope(stats, thread_name, stream, child_scope)?;
        ns_used_by_children += child_scope.record.duration_ns;
    }

    let self_time = scope.record.duration_ns.saturating_sub(ns_used_by_children);

    let key = Key {
        id: scope.id,
        thread_name: thread_name.to_owned(),
    };
    let scope_stats = stats.scopes.entry(key).or_default();
    scope_stats.count += 1;
    scope_stats.bytes += scope_byte_size(scope);
    scope_stats.total_self_ns += self_time;
    scope_stats.max_ns = scope_stats.max_ns.max(self_time);

    Ok(())
}

fn scope_byte_size(scope: &puffin::Scope<'_>) -> usize {
    1 + // `(` sentinel
    8 + // start time
    8 + // scope id
    1 + scope.record.data.len() + // dynamic data len
    8 + // scope size
    1 + // `)` sentinel
    8 // stop time
}

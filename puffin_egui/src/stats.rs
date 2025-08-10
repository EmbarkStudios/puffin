use egui::TextBuffer;
use puffin::*;

use crate::filter::Filter;

#[derive(Clone, Debug, Default)]
pub struct Options {
    filter: Filter,
}

#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum SortKey {
    Location,
    FunctionName,
    ScopeName,
    Count,
    Size,
    TotalSelfTime,
    MeanSelfTime,
    MaxSelfTime,
}

/// Determines the order of scopes in table view.
#[derive(Copy, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct SortOrder {
    /// Which column to sort scopes by
    pub key: SortKey,

    /// Reverse order, if true sort in descending order.
    /// If false sort in ascending order.
    pub rev: bool,
}

impl SortOrder {
    fn sort_scopes(&self, scopes: &mut [(&Key, ScopeStats)], scope_infos: &ScopeCollection) {
        match self.key {
            SortKey::Location => {
                scopes.sort_by_key(|(key, _scope_stats)| {
                    if let Some(scope_details) = scope_infos.fetch_by_id(&key.id) {
                        scope_details.location()
                    } else {
                        String::new()
                    }
                });
            }
            SortKey::FunctionName => {
                scopes.sort_by_key(|(key, _scope_stats)| {
                    if let Some(scope_details) = scope_infos.fetch_by_id(&key.id) {
                        scope_details.function_name.as_str()
                    } else {
                        ""
                    }
                });
            }
            SortKey::ScopeName => {
                scopes.sort_by_key(|(key, _scope_stats)| {
                    if let Some(scope_details) = scope_infos.fetch_by_id(&key.id) {
                        if let Some(name) = &scope_details.scope_name {
                            name.as_ref()
                        } else {
                            ""
                        }
                    } else {
                        ""
                    }
                });
            }
            SortKey::Count => {
                scopes.sort_by_key(|(_key, scope_stats)| scope_stats.count);
            }
            SortKey::Size => {
                scopes.sort_by_key(|(_key, scope_stats)| scope_stats.bytes);
            }
            SortKey::TotalSelfTime => {
                scopes.sort_by_key(|(_key, scope_stats)| scope_stats.total_self_ns);
            }
            SortKey::MeanSelfTime => {
                scopes.sort_by_key(|(_key, scope_stats)| {
                    scope_stats.total_self_ns as usize / scope_stats.count
                });
            }
            SortKey::MaxSelfTime => {
                scopes.sort_by_key(|(_key, scope_stats)| scope_stats.max_ns);
            }
        }
        if self.rev {
            scopes.reverse();
        }
    }

    fn get_arrow(&self) -> &str {
        if self.rev { "⏷" } else { "⏶" }
    }

    fn toggle(&mut self) {
        self.rev = !self.rev;
    }
}

fn header_label(ui: &mut egui::Ui, name: &str, sort_key: SortKey, sort_order: &mut SortOrder) {
    if sort_order.key == sort_key {
        if ui
            .strong(format!("{} {}", name, sort_order.get_arrow()))
            .clicked()
        {
            sort_order.toggle();
        }
    } else {
        if ui.strong(name).clicked() {
            *sort_order = SortOrder {
                key: sort_key,
                rev: true,
            }
        }
    }
}

pub fn ui(
    ui: &mut egui::Ui,
    options: &mut Options,
    scope_infos: &ScopeCollection,
    frames: &[std::sync::Arc<UnpackedFrameData>],
    sort_order: &mut SortOrder,
) {
    let mut threads = std::collections::HashSet::<&ThreadInfo>::new();
    let mut stats = Stats::default();

    for frame in frames {
        threads.extend(frame.thread_streams.keys());
        for stream in frame.thread_streams.values() {
            collect_stream(&mut stats, &stream.stream).ok();
        }
    }

    let mut total_bytes = 0;
    let mut total_ns = 0;
    for scope in stats.scopes.values() {
        total_bytes += scope.bytes;
        total_ns += scope.total_self_ns;
    }

    ui.label("This view can be used to find functions that are called a lot.\n\
              The overhead of a profile scope is around ~50ns, so remove profile scopes from fast functions that are called often.");

    ui.label(format!(
        "Currently viewing {} unique scopes, using a total of {:.1} kB, covering {:.1} ms over {} thread(s)",
        stats.scopes.len(),
        total_bytes as f32 * 1e-3,
        total_ns as f32 * 1e-6,
        threads.len()
    ));

    options.filter.ui(ui);

    let mut scopes: Vec<_> = stats
        .scopes
        .iter()
        .map(|(key, value)| (key, *value))
        .collect();
    scopes.sort_by_key(|(key, _)| *key);
    sort_order.sort_scopes(&mut scopes, scope_infos);

    egui::ScrollArea::horizontal().show(ui, |ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        ui.spacing_mut().item_spacing.x = 16.0;

        egui_extras::TableBuilder::new(ui)
            .striped(true)
            .columns(
                egui_extras::Column::auto_with_initial_suggestion(200.0).resizable(true),
                3,
            )
            .columns(egui_extras::Column::auto().resizable(false), 6)
            .header(20.0, |mut header| {
                header.col(|ui| {
                    header_label(ui, "Location", SortKey::Location, sort_order);
                });
                header.col(|ui| {
                    header_label(ui, "Function Name", SortKey::FunctionName, sort_order);
                });
                header.col(|ui| {
                    header_label(ui, "Scope Name", SortKey::ScopeName, sort_order);
                });
                header.col(|ui| {
                    header_label(ui, "Count", SortKey::Count, sort_order);
                });
                header.col(|ui| {
                    header_label(ui, "Size", SortKey::Size, sort_order);
                });
                header.col(|ui| {
                    header_label(ui, "Total self time", SortKey::TotalSelfTime, sort_order);
                });
                header.col(|ui| {
                    header_label(ui, "Mean self time", SortKey::MeanSelfTime, sort_order);
                });
                header.col(|ui| {
                    header_label(ui, "Max self time", SortKey::MaxSelfTime, sort_order);
                });
            })
            .body(|mut body| {
                for (key, stats) in &scopes {
                    let Some(scope_details) = scope_infos.fetch_by_id(&key.id) else {
                        continue;
                    };

                    if !options.filter.is_empty() {
                        let mut matches = options.filter.include(&scope_details.function_name);

                        if let Some(scope_name) = &scope_details.scope_name {
                            matches |= options.filter.include(scope_name);
                        }

                        if !matches {
                            continue;
                        }
                    }

                    body.row(14.0, |mut row| {
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
                            let color = if stats.count < 1_000 {
                                ui.visuals().text_color()
                            } else if stats.count < 10_000 {
                                ui.visuals().warn_fg_color
                            } else {
                                ui.visuals().error_fg_color
                            };

                            ui.label(
                                egui::RichText::new(format!("{:>5}", stats.count))
                                    .monospace()
                                    .color(color),
                            );
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

fn collect_stream(stats: &mut Stats, stream: &puffin::Stream) -> puffin::Result<()> {
    for scope in puffin::Reader::from_start(stream) {
        collect_scope(stats, stream, &scope?)?;
    }
    Ok(())
}

fn collect_scope<'s>(
    stats: &mut Stats,
    stream: &'s puffin::Stream,
    scope: &puffin::Scope<'s>,
) -> puffin::Result<()> {
    let mut ns_used_by_children = 0;
    for child_scope in Reader::with_offset(stream, scope.child_begin_position)? {
        let child_scope = &child_scope?;
        collect_scope(stats, stream, child_scope)?;
        ns_used_by_children += child_scope.record.duration_ns;
    }

    let self_time = scope.record.duration_ns.saturating_sub(ns_used_by_children);

    let key = Key { id: scope.id };
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

use puffin::*;

pub fn ui(ui: &mut egui::Ui, frame: &FrameData) {
    let mut stats = Stats::default();
    for stream in frame.thread_streams.values() {
        collect_stream(&mut stats, &stream.stream).ok();
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
        frame.thread_streams.len()
    ));

    ui.separator();

    let mut scopes: Vec<_> = stats
        .scopes
        .iter()
        .map(|(key, value)| (*key, *value))
        .collect();
    scopes.sort_by_key(|(id_loc, _)| *id_loc);
    scopes.sort_by_key(|(_id_loc, scope_stats)| scope_stats.bytes);
    scopes.reverse();

    egui::ScrollArea::auto_sized().show(ui, |ui| {
        egui::Grid::new("table")
            .spacing([32.0, ui.spacing().item_spacing.y])
            .show(ui, |ui| {
                ui.heading("Location");
                ui.heading("ID");
                ui.heading("Count");
                ui.heading("Size");
                ui.heading("Total self time");
                ui.heading("Mean self time");
                ui.heading("Max self time");
                ui.end_row();

                for (id_loc, stats) in &scopes {
                    ui.label(id_loc.location);
                    ui.label(id_loc.id);
                    ui.monospace(format!("{:>5}", stats.count));
                    ui.monospace(format!("{:>6.1} kB", stats.bytes as f32 * 1e-3));
                    ui.monospace(format!("{:>8.1} µs", stats.total_self_ns as f32 * 1e-3));
                    ui.monospace(format!(
                        "{:>8.1} µs",
                        stats.total_self_ns as f32 * 1e-3 / (stats.count as f32)
                    ));
                    ui.monospace(format!("{:>8.1} µs", stats.max_ns as f32 * 1e-3));
                    ui.end_row();
                }
            });
    });
}

#[derive(Default)]
struct Stats<'s> {
    scopes: std::collections::HashMap<IdAndLocation<'s>, ScopeStats>,
}

#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct IdAndLocation<'s> {
    id: &'s str,
    location: &'s str,
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

fn collect_stream<'s>(stats: &mut Stats<'s>, stream: &'s puffin::Stream) -> puffin::Result<()> {
    for scope in puffin::Reader::from_start(stream) {
        collect_scope(stats, stream, &scope?)?;
    }
    Ok(())
}

fn collect_scope<'s>(
    stats: &mut Stats<'s>,
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

    let key = IdAndLocation {
        id: scope.record.id,
        location: scope.record.location,
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
    1 + scope.record.id.len() + //
    1 + scope.record.location.len() + //
    1 + scope.record.data.len() + //
    8 + // scope size
    1 + // `)` sentinel
    8 // stop time
}

use criterion::{criterion_group, criterion_main, Criterion};

pub fn criterion_benchmark(c: &mut Criterion) {
    puffin::set_scopes_on(true);

    c.bench_function("profile_function", |b| {
        puffin::GlobalProfiler::lock().new_frame();
        puffin::profile_scope!("keep one scope open so we don't profile sending scopes");
        b.iter(|| {
            puffin::profile_function!();
        })
    });
    c.bench_function("profile_function_data", |b| {
        puffin::GlobalProfiler::lock().new_frame();
        puffin::profile_scope!("keep one scope open so we don't profile sending scopes");
        b.iter(|| {
            puffin::profile_function!("my_mesh.obj");
        })
    });
    c.bench_function("profile_scope", |b| {
        puffin::GlobalProfiler::lock().new_frame();
        puffin::profile_scope!("keep one scope open so we don't profile sending scopes");
        b.iter(|| {
            puffin::profile_scope!("my longish scope name");
        })
    });
    c.bench_function("profile_scope_data", |b| {
        puffin::GlobalProfiler::lock().new_frame();
        puffin::profile_scope!("keep one scope open so we don't profile sending scopes");
        b.iter(|| {
            puffin::profile_scope!("my longish scope name", "my_mesh.obj");
        })
    });
    c.bench_function("flush_frames", |b| {
        puffin::GlobalProfiler::lock().new_frame();
        let _fv = puffin::GlobalFrameView::default();

        b.iter(|| {
            puffin::profile_function!();
            puffin::GlobalProfiler::lock().new_frame();
        })
    });

    puffin::set_scopes_on(false);
    c.bench_function("profile_function_off", |b| {
        puffin::GlobalProfiler::lock().new_frame();
        puffin::profile_scope!("keep one scope open so we don't profile sending scopes");
        b.iter(|| {
            puffin::profile_function!();
        })
    });
    c.bench_function("profile_function_data_off", |b| {
        puffin::GlobalProfiler::lock().new_frame();
        puffin::profile_scope!("keep one scope open so we don't profile sending scopes");
        b.iter(|| {
            puffin::profile_function!("my_mesh.obj");
        })
    });
    c.bench_function("profile_scope_off", |b| {
        puffin::GlobalProfiler::lock().new_frame();
        puffin::profile_scope!("keep one scope open so we don't profile sending scopes");
        b.iter(|| {
            puffin::profile_scope!("my longish scope name");
        })
    });
    c.bench_function("profile_scope_data_off", |b| {
        puffin::GlobalProfiler::lock().new_frame();
        puffin::profile_scope!("keep one scope open so we don't profile sending scopes");
        b.iter(|| {
            puffin::profile_scope!("my longish scope name", "my_mesh.obj");
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);

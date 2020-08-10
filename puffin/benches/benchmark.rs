use criterion::{criterion_group, criterion_main, Criterion};

pub fn criterion_benchmark(c: &mut Criterion) {
    puffin::set_scopes_on(true);
    puffin::profile_scope!("keep one scope open so we don't profile sending scopes");

    c.bench_function("profile_function", |b| {
        b.iter(|| {
            puffin::profile_function!();
        })
    });
    c.bench_function("profile_function_data", |b| {
        b.iter(|| {
            puffin::profile_function_data!("my_mesh.obj");
        })
    });
    c.bench_function("profile_scope", |b| {
        b.iter(|| {
            puffin::profile_scope!("my longish scope name");
        })
    });
    c.bench_function("profile_scope_data", |b| {
        b.iter(|| {
            puffin::profile_scope_data!("my longish scope name", "my_mesh.obj");
        })
    });

    puffin::set_scopes_on(false);
    c.bench_function("profile_function_off", |b| {
        b.iter(|| {
            puffin::profile_function!();
        })
    });
    c.bench_function("profile_function_data_off", |b| {
        b.iter(|| {
            puffin::profile_function_data!("my_mesh.obj");
        })
    });
    c.bench_function("profile_scope_off", |b| {
        b.iter(|| {
            puffin::profile_scope!("my longish scope name");
        })
    });
    c.bench_function("profile_scope_data_off", |b| {
        b.iter(|| {
            puffin::profile_scope_data!("my longish scope name", "my_mesh.obj");
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);

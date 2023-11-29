use criterion::{criterion_group, criterion_main, Criterion};
pub fn custom_benchark<F: Fn()>(name: &str, cb: F) {
    const RUNS: usize = 5000;
    const SCOPES: usize = 500;

    let mut timings = vec![];
    let mut averages = vec![];

    puffin::profile_function!();

    let mut run_benchmark = || {
        puffin::GlobalProfiler::lock().new_frame();

        for _i in 0..SCOPES {
            let start_time = std::time::Instant::now();

            // Run three times as firt time
            cb();

            timings.push(start_time.elapsed());
        }
    };

    for _i in 0..RUNS {
        run_benchmark();
    }

    for chunk in timings.chunks(20) {
        let nanos: u128 = chunk.iter().map(|x|x.as_nanos()).sum::<u128>() / chunk.len() as u128;
        averages.push(nanos);
    }

    let average_avergage: u128 = averages.iter().sum::<u128>() / averages.len() as u128;

    println!("[{name}]: Ran {RUNS} of {SCOPES} scopes. Average of {} calls is {}ns per call", RUNS * SCOPES, average_avergage);
}

fn run_custom_benchark() {
    custom_benchark("profile_function! macro", || {puffin::profile_function!();});
    custom_benchark("profile_scope! macro", || {puffin::profile_scope!("scope");});
    custom_benchark("`profile_function!` with data macro", || {puffin::profile_function!("data");});
    custom_benchark("`profile_scope!` with data macro", || {puffin::profile_scope!("scope", "data");});
}

pub fn criterion_benchmark(c: &mut Criterion) {
    puffin::set_scopes_on(true);

    run_custom_benchark();

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

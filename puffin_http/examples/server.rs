#[expect(clippy::unwrap_used)]
#[expect(clippy::print_stderr)]
#[expect(clippy::infinite_loop)]
fn main() {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .without_timestamps()
        .init()
        .ok();

    let server_addr = format!("localhost:{}", puffin_http::DEFAULT_PORT);
    let mut puffin_server = puffin_http::Server::new(&server_addr).unwrap();
    eprintln!(
        "Serving demo profile data on {}. Run `puffin_viewer --url \"{}\"` to view it.",
        server_addr,
        puffin_server.local_addr()
    );
    puffin_server.set_on_state_change(|has_clients| {
        puffin::set_scopes_on(has_clients);
        if has_clients {
            eprintln!("Profiling enabled");
        } else {
            eprintln!("Profiling disabled");
        }
    });
    let _puffin_server = puffin_server;

    let mut frame_counter = 0;

    loop {
        puffin::profile_scope!("main_loop", format!("frame {frame_counter}"));
        puffin::GlobalProfiler::lock().new_frame();

        // Give us something to inspect:

        std::thread::Builder::new()
            .name("Other thread".to_owned())
            .spawn(|| {
                sleep_ms(5);
            })
            .unwrap();

        sleep_ms(14);
        if frame_counter % 7 == 0 {
            puffin::profile_scope!("Spike");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        for _ in 0..1000 {
            puffin::profile_scope!("very thin");
        }

        frame_counter += 1;
    }
}

fn sleep_ms(ms: usize) {
    puffin::profile_function!();
    match ms {
        0 => {}
        1 => std::thread::sleep(std::time::Duration::from_millis(1)),
        _ => {
            sleep_ms(ms / 2);
            sleep_ms(ms - (ms / 2));
        }
    }
}

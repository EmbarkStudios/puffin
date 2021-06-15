fn main() {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()
        .ok();

    let server_addr = format!("0.0.0.0:{}", puffin_http::DEFAULT_PORT);
    eprintln!("Serving demo profile data on {}", server_addr);

    let puffin_server = puffin_http::Server::new(&server_addr).unwrap();

    puffin::set_scopes_on(true);

    loop {
        puffin::profile_scope!("main_loop");
        puffin::GlobalProfiler::lock().new_frame();
        puffin_server.update();

        sleep_ms(16);
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

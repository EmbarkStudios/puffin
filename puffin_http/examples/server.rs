fn main() {
    stderrlog::new()
        .module(module_path!())
        .verbosity(2) // 2 == info
        .init()
        .unwrap();

    let server_addr = format!("localhost:{}", puffin_http::DEFAULT_PORT);
    eprintln!("Serving demo profile data on {}", server_addr);

    let puffin_server = puffin_http::PuffinServer::new(&server_addr).unwrap();

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

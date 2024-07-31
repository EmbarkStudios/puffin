use eframe::egui;

fn main() -> eframe::Result<()> {
    let mut frame_counter = 0;
    let mut keep_repainting = true;

    puffin::set_scopes_on(true);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
        ..Default::default()
    };
    eframe::run_simple_native("puffin egui eframe", options, move |ctx, _frame| {
        puffin::profile_function!();
        puffin::GlobalProfiler::lock().new_frame(); // If you use the `puffin` feature of `eframe` you don't need to call this

        egui::CentralPanel::default().show(ctx, |ui| {
            let mut profile = puffin::are_scopes_on();
            ui.checkbox(&mut profile, "Show profiler window");
            puffin::set_scopes_on(profile); // controls both the profile capturing, and the displaying of it

            ui.horizontal(|ui| {
                ui.checkbox(&mut keep_repainting, "Keep repainting this window");
                if keep_repainting {
                    ui.spinner();
                    ui.ctx().request_repaint();
                }
            });

            if ui.button("Quit").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });

        // This call does nothing if profiling is disabled
        puffin_egui::show_viewport_if_enabled(ctx);

        // ----------------------------------------------------------------

        // Give us something to inspect:
        std::thread::Builder::new()
            .name("Other thread".to_owned())
            .spawn(|| {
                sleep_ms(5);
            })
            .unwrap();

        sleep_ms(9);
        if frame_counter % 49 == 0 {
            puffin::profile_scope!("Spike");
            std::thread::sleep(std::time::Duration::from_millis(20))
        }
        if frame_counter % 343 == 0 {
            puffin::profile_scope!("Big spike");
            std::thread::sleep(std::time::Duration::from_millis(50))
        }
        if frame_counter % 55 == 0 {
            // test to verify these spikes timers are not merged together as they have different data
            for (name, ms) in [("First".to_string(), 20), ("Second".to_string(), 15)] {
                puffin::profile_scope!("Spike", name);
                std::thread::sleep(std::time::Duration::from_millis(ms))
            }
            // these are however fine to merge together as data is the same
            for (_name, ms) in [("First".to_string(), 20), ("Second".to_string(), 15)] {
                puffin::profile_scope!("Spike");
                std::thread::sleep(std::time::Duration::from_millis(ms))
            }
        }

        for _ in 0..1000 {
            puffin::profile_scope!("very thin");
        }

        frame_counter += 1;
    })
}

fn sleep_ms(ms: usize) {
    puffin::profile_function_if!(ms > 1);
    match ms {
        0 => {}
        1 => std::thread::sleep(std::time::Duration::from_millis(1)),
        _ => {
            sleep_ms(ms / 2);
            sleep_ms(ms - (ms / 2));
        }
    }
}

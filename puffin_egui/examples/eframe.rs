use eframe::egui;

fn main() {
    puffin::set_scopes_on(true); // Remember to call this, or puffin will be disabled!

    let native_options = Default::default();
    let _ = eframe::run_native(
        "puffin egui eframe",
        native_options,
        Box::new(|_cc| Box::<ExampleApp>::default()),
    );
}

#[derive(Default)]
pub struct ExampleApp {
    frame_counter: u64,
}

impl eframe::App for ExampleApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        puffin::profile_function!();
        puffin::GlobalProfiler::lock().new_frame(); // call once per frame!

        puffin_egui::profiler_window(ctx);

        // Give us something to inspect:

        std::thread::Builder::new()
            .name("Other thread".to_owned())
            .spawn(|| {
                sleep_ms(5);
            })
            .unwrap();

        sleep_ms(14);
        if self.frame_counter % 49 == 0 {
            puffin::profile_scope!("Spike");
            std::thread::sleep(std::time::Duration::from_millis(20))
        }
        if self.frame_counter % 343 == 0 {
            puffin::profile_scope!("Big spike");
            std::thread::sleep(std::time::Duration::from_millis(50))
        }
        if self.frame_counter % 55 == 0 {
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

        self.frame_counter += 1;
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

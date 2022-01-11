use eframe::{egui, epi};

fn main() {
    puffin::set_scopes_on(true); // Remember to call this, or puffin will be disabled!

    let app = ExampleApp::default();
    let options = Default::default();
    eframe::run_native(Box::new(app), options);
}

#[derive(Default)]
pub struct ExampleApp {
    frame_counter: u64,
}

impl epi::App for ExampleApp {
    fn name(&self) -> &str {
        "puffin egui eframe"
    }

    fn update(&mut self, ctx: &egui::CtxRef, _frame: &epi::Frame) {
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

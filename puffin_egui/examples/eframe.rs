use eframe::{egui, epi};

fn main() {
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

    fn setup(&mut self, _ctx: &egui::CtxRef) {
        puffin::set_scopes_on(true); // Remember to call this, or puffin will be disabled!
    }

    fn update(&mut self, ctx: &egui::CtxRef, _frame: &mut epi::Frame<'_>) {
        puffin::profile_function!();
        puffin::GlobalProfiler::lock().new_frame(); // call once per frame!

        puffin_egui::profiler_window(ctx);

        // Give us something to inspect:
        sleep_ms(14);
        if self.frame_counter % 7 == 0 {
            puffin::profile_scope!("Spike");
            std::thread::sleep(std::time::Duration::from_millis(10))
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

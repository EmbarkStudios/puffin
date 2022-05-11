use eframe::egui;
use puffin_tracing::PuffinLayer;
use tracing::info_span;
use tracing_subscriber::{layer::SubscriberExt, Registry};

fn main() {
    let subscriber = Registry::default().with(PuffinLayer::new());
    tracing::subscriber::set_global_default(subscriber).unwrap();

    puffin::set_scopes_on(true); // Remember to call this, or puffin will be disabled!

    let native_options = Default::default();
    eframe::run_native(
        "puffin egui eframe",
        native_options,
        Box::new(|_cc| Box::new(ExampleApp::default())),
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
            let _span = info_span!("Spike", millis = 20).entered();
            std::thread::sleep(std::time::Duration::from_millis(20))
        }
        if self.frame_counter % 343 == 0 {
            let _span = info_span!("Big spike", millis = 50).entered();
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

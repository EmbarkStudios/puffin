use eframe::wasm_bindgen::{self, prelude::*};

/// This is the entry-point for all the web-assembly.
/// This is called once from the HTML.
/// It loads the app, installs some callbacks, then returns.
/// You can add more callbacks like this if you want to call in to your code.
#[allow(clippy::unused_unit)]
#[wasm_bindgen]
pub async fn start(canvas_id: &str) -> Result<(), eframe::wasm_bindgen::JsValue> {
    puffin::set_scopes_on(true); // quiet warning in `puffin_egui`.

    // Redirect [`log`] message to `console.log` and friends:
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    let web_options = eframe::WebOptions::default();
    let runner = eframe::WebRunner::new();
    runner
        .start(
            canvas_id,
            web_options,
            Box::new(|cc| {
                Ok(Box::new(crate::PuffinViewer::new(
                    crate::Source::None,
                    cc.storage,
                )))
            }),
        )
        .await?;

    Ok(())
}

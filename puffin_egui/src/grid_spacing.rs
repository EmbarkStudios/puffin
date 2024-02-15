use std::str::FromStr;

const DEFAULT_SPACING_MS: i64 = 1;

#[derive(Clone, Debug)]
pub struct GridSpacing {
    text: String,
}

impl Default for GridSpacing {
    fn default() -> Self {
        Self {
            text: DEFAULT_SPACING_MS.to_string(),
        }
    }
}

impl GridSpacing {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Grid Spacing (ms):");
            ui.text_edit_singleline(&mut self.text);

            // Only allow 0-9 and a single ".".
            let mut decimal_point_found = false;
            self.text.retain(|c| {
                if c == '.' && !decimal_point_found {
                    decimal_point_found = true;
                    true
                } else {
                    c.is_ascii_digit()
                }
            });

            if ui.button("ｘ").clicked() {
                self.text = DEFAULT_SPACING_MS.to_string();
            }
        });
    }

    pub fn grid_spacing_ns(&self) -> i64 {
        let grid_spacing_ms = f64::from_str(&self.text).unwrap_or(DEFAULT_SPACING_MS as f64);
        (grid_spacing_ms * 1_000.).round() as i64
    }
}

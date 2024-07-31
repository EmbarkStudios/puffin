#[derive(Clone, Debug, Default)]
pub struct Filter {
    filter: String,
}

impl Filter {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Scope filter"));
            self.filter = self.filter.to_lowercase();

            if ui.button("ｘ").clicked() {
                self.filter.clear();
            }
        });
    }

    /// if true, show everything
    pub fn is_empty(&self) -> bool {
        self.filter.is_empty()
    }

    pub fn include(&self, id: &str) -> bool {
        if self.filter.is_empty() {
            true
        } else {
            id.to_lowercase().contains(&self.filter)
        }
    }

    pub fn set_filter(&mut self, filter: String) {
        self.filter = filter;
    }
}

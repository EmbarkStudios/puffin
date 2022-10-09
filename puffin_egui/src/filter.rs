#[derive(Clone, Debug, Default)]
pub struct Filter {
    filter: String,
}

impl Filter {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Scope filter:");
            ui.text_edit_singleline(&mut self.filter);
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

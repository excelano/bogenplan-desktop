// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use bogenplan_store::Store;
use eframe::egui::{self, Align, Layout, RichText, TextEdit, Ui};

use crate::editor::Pending;
use crate::import::heading;
use crate::palette;

pub struct Subscribe {
    address: String,
    pending: Pending,
}

impl Subscribe {
    pub fn new(address: String) -> Subscribe {
        Subscribe {
            address,
            pending: Pending::new(),
        }
    }

    pub fn field(&mut self, ui: &mut Ui, store: &Store) {
        if self.pending.poll(ui) {
            self.address.clear();
        }
        ui.horizontal(|ui| {
            ui.add(
                TextEdit::singleline(&mut self.address)
                    .hint_text("webcal:// or https:// address")
                    .desired_width(260.0),
            );
            let ready = !self.address.trim().is_empty() && !self.pending.busy();
            if ui
                .add_enabled(ready, egui::Button::new("Subscribe"))
                .clicked()
            {
                self.pending
                    .wait(store.subscribe(self.address.trim().to_string()));
            }
            if self.pending.busy() {
                ui.spinner();
            }
        });
        self.pending.show(ui);
    }

    pub fn sheet(&mut self, ui: &mut Ui, store: &Store) -> bool {
        if self.pending.poll(ui) {
            return true;
        }
        ui.set_width(380.0);
        heading(ui, "Subscribe", &host(&self.address));
        ui.label(RichText::new(&self.address).size(11.0).monospace());
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "Its events are shown read-only and fetched again every hour. The address is kept in the GNOME keyring.",
            )
            .size(11.0)
            .color(palette::LABEL),
        );
        self.pending.show(ui);
        ui.add_space(8.0);
        let mut closed = false;
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add_enabled(!self.pending.busy(), egui::Button::new("Subscribe"))
                .clicked()
            {
                self.pending.wait(store.subscribe(self.address.clone()));
            }
            if ui.button("Cancel").clicked() {
                closed = true;
            }
            if self.pending.busy() {
                ui.spinner();
            }
        });
        closed
    }
}

fn host(address: &str) -> String {
    url::Url::parse(address)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| address.to_string())
}

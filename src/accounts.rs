// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use bogenplan_store::{Reply, Store};
use eframe::egui::{self, RichText, TextEdit, Ui};

use crate::palette;

pub const ICLOUD: &str = "caldav.icloud.com";

pub struct SignIn {
    server: String,
    username: String,
    password: String,
    pending: Option<Reply>,
    failure: Option<String>,
}

impl Default for SignIn {
    fn default() -> SignIn {
        SignIn {
            server: ICLOUD.to_string(),
            username: String::new(),
            password: String::new(),
            pending: None,
            failure: None,
        }
    }
}

impl SignIn {
    pub fn show(&mut self, ui: &mut Ui, store: &Store) {
        if let Some(reply) = &self.pending {
            match reply.try_recv() {
                Ok(Ok(())) => *self = SignIn::default(),
                Ok(Err(error)) => {
                    self.failure = Some(error);
                    self.pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ui
                    .ctx()
                    .request_repaint_after(std::time::Duration::from_millis(200)),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.pending = None,
            }
        }
        egui::Grid::new("sign in")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("Server");
                ui.add(TextEdit::singleline(&mut self.server).desired_width(260.0));
                ui.end_row();
                ui.label(if self.server == ICLOUD {
                    "Apple Account"
                } else {
                    "User name"
                });
                ui.add(TextEdit::singleline(&mut self.username).desired_width(260.0));
                ui.end_row();
                ui.label(if self.server == ICLOUD {
                    "App-specific password"
                } else {
                    "Password"
                });
                ui.add(
                    TextEdit::singleline(&mut self.password)
                        .password(true)
                        .desired_width(260.0),
                );
                ui.end_row();
            });
        if self.server == ICLOUD {
            ui.label(
                RichText::new("Make an app-specific password at account.apple.com, under Sign-In and Security.")
                    .size(11.0)
                    .color(palette::LABEL),
            );
        }
        if let Some(failure) = &self.failure {
            ui.label(
                RichText::new(failure)
                    .size(12.0)
                    .color(egui::Color32::from_rgb(0xFF, 0x8A, 0x80)),
            );
        }
        ui.horizontal(|ui| {
            let ready = !self.server.trim().is_empty()
                && !self.username.trim().is_empty()
                && !self.password.trim().is_empty();
            if ui
                .add_enabled(
                    ready && self.pending.is_none(),
                    egui::Button::new("Sign In"),
                )
                .clicked()
            {
                self.failure = None;
                self.pending = Some(store.sign_in(
                    self.server.trim().to_string(),
                    self.username.trim().to_string(),
                    self.password.trim().to_string(),
                ));
            }
            if self.pending.is_some() {
                ui.spinner();
            }
        });
    }
}

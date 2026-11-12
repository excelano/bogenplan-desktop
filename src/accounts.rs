// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use bogenplan_store::Store;
use eframe::egui::{self, RichText, TextEdit, Ui};

use crate::editor::Pending;
use crate::palette;

const ICLOUD: &str = "caldav.icloud.com";

struct Service {
    name: &'static str,
    server: Option<&'static str>,
    username: &'static str,
    password: &'static str,
    note: Option<&'static str>,
}

const SERVICES: [Service; 6] = [
    Service {
        name: "iCloud",
        server: Some(ICLOUD),
        username: "Apple Account",
        password: "App-specific password",
        note: Some(
            "Make an app-specific password at account.apple.com, under Sign-In and Security.",
        ),
    },
    Service {
        name: "Fastmail",
        server: Some("caldav.fastmail.com"),
        username: "Email address",
        password: "App password",
        note: Some("Make an app password with calendar access in Fastmail's settings."),
    },
    Service {
        name: "mailbox.org",
        server: Some("dav.mailbox.org"),
        username: "Email address",
        password: "Password",
        note: None,
    },
    Service {
        name: "Posteo",
        server: Some("posteo.de:8443"),
        username: "Email address",
        password: "Password",
        note: None,
    },
    Service {
        name: "Nextcloud",
        server: None,
        username: "User name",
        password: "App password",
        note: Some(
            "The server is your Nextcloud's address; if it is not found, add /remote.php/dav to it. Make an app password in Nextcloud under Settings, Security.",
        ),
    },
    Service {
        name: "Other CalDAV server",
        server: None,
        username: "User name",
        password: "Password",
        note: None,
    },
];

pub fn service_name(server: &str) -> Option<&'static str> {
    SERVICES
        .iter()
        .find(|service| service.server == Some(server))
        .map(|service| service.name)
}

pub struct SignIn {
    service: usize,
    server: String,
    username: String,
    password: String,
    pending: Pending,
}

impl Default for SignIn {
    fn default() -> SignIn {
        SignIn {
            service: 0,
            server: String::new(),
            username: String::new(),
            password: String::new(),
            pending: Pending::new(),
        }
    }
}

impl SignIn {
    pub fn show(&mut self, ui: &mut Ui, store: &Store) {
        if self.pending.poll(ui) {
            *self = SignIn::default();
        }
        let service = &SERVICES[self.service];
        egui::Grid::new("sign in")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("Service");
                egui::ComboBox::from_id_salt("service")
                    .selected_text(service.name)
                    .show_ui(ui, |ui| {
                        for (index, choice) in SERVICES.iter().enumerate() {
                            ui.selectable_value(&mut self.service, index, choice.name);
                        }
                    });
                ui.end_row();
                if service.server.is_none() {
                    ui.label("Server");
                    ui.add(
                        TextEdit::singleline(&mut self.server)
                            .hint_text("caldav.example.com")
                            .desired_width(260.0),
                    );
                    ui.end_row();
                }
                ui.label(service.username);
                ui.add(TextEdit::singleline(&mut self.username).desired_width(260.0));
                ui.end_row();
                ui.label(service.password);
                ui.add(
                    TextEdit::singleline(&mut self.password)
                        .password(true)
                        .desired_width(260.0),
                );
                ui.end_row();
            });
        if let Some(note) = service.note {
            ui.label(RichText::new(note).size(11.0).color(palette::LABEL));
        }
        self.pending.show(ui);
        let server = service.server.unwrap_or(self.server.trim()).to_string();
        ui.horizontal(|ui| {
            let ready = !server.is_empty()
                && !self.username.trim().is_empty()
                && !self.password.trim().is_empty();
            if ui
                .add_enabled(ready && !self.pending.busy(), egui::Button::new("Sign In"))
                .clicked()
            {
                self.pending.wait(store.sign_in(
                    server,
                    self.username.trim().to_string(),
                    self.password.trim().to_string(),
                ));
            }
            if self.pending.busy() {
                ui.spinner();
            }
        });
    }
}

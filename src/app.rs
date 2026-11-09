// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::HashMap;
use std::time::Duration;

use bogenplan_store::{Calendar, CalendarSource, Occurrence, Status, Store};
use chrono::{DateTime, NaiveDate, Utc, Weekday};
use eframe::egui::{
    self, Align, Key, KeyboardShortcut, Layout, Modifiers, RichText, ScrollArea, Ui,
};

use crate::accounts::SignIn;
use crate::allday::{self, Picked};
use crate::axis::DayAxis;
use crate::civil::{Clock, date_of, day_of, first_weekday, naive, week_start};
use crate::day::{self, Action, Colors, Column, DayView, Probe};
use crate::demo::{self, Demo};
use crate::detail;
use crate::display::{Precision, Readout, Scale};
use crate::glyph::Label;
use crate::moment::{Draft, Outcome};
use crate::palette;
use crate::place::Place;
use crate::settings::Settings;
use crate::system;
use crate::time::instant;
use crate::week;

const QUIT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Q);
const SETTINGS: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Comma);
const REFRESH: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::F5);
const MOMENT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
const OPENING_MARGIN: f32 = 160.0;
const HEADER_MARGIN: f32 = 12.0;

pub enum Source {
    Live(Store),
    Demo(Demo),
}

impl Source {
    fn calendars(&self) -> &dyn CalendarSource {
        match self {
            Source::Live(store) => store,
            Source::Demo(demo) => demo,
        }
    }
}

pub struct App {
    source: Source,
    settings: Settings,
    clock: Clock,
    first_weekday: Weekday,
    today: i64,
    selection: i64,
    page: Option<(i64, usize)>,
    scroll_to: Option<f32>,
    scroll_offset: f32,
    visible_height: f32,
    day_rect: egui::Rect,
    probe: Option<Probe>,
    detail: Option<Occurrence>,
    all_day: Option<Vec<Occurrence>>,
    moment: Option<Draft>,
    settings_open: bool,
    sign_in: SignIn,
    latitude: String,
    longitude: String,
    #[cfg(feature = "screenshot")]
    screenshot: Option<crate::screenshot::Screenshot>,
}

impl App {
    pub fn new(source: Source, opening: Option<NaiveDate>) -> App {
        let mut settings = Settings::load();
        let demo = matches!(source, Source::Demo(_));
        if demo {
            settings = Settings {
                place: Some(demo::PLACE),
                ..Settings::default()
            };
        }
        let mut app = App {
            source,
            clock: if demo {
                Clock::TwelveHour
            } else {
                system::clock()
            },
            first_weekday: first_weekday(),
            today: 0,
            selection: 0,
            page: None,
            scroll_to: None,
            scroll_offset: 0.0,
            visible_height: 600.0,
            day_rect: egui::Rect::NOTHING,
            probe: None,
            detail: None,
            all_day: None,
            moment: None,
            settings_open: false,
            sign_in: SignIn::default(),
            latitude: settings
                .place
                .map(|place| format!("{:.4}", place.latitude))
                .unwrap_or_default(),
            longitude: settings
                .place
                .map(|place| format!("{:.4}", place.longitude))
                .unwrap_or_default(),
            settings,
            #[cfg(feature = "screenshot")]
            screenshot: crate::screenshot::Screenshot::from_environment(),
        };
        if let Some(place) = app.settings.place {
            app.today = app.today_at(place);
            app.selection = opening.map_or(app.today, day_of);
        }
        app
    }

    pub fn set_zoom(&mut self, points_per_span: f32) {
        self.settings.points_per_span =
            points_per_span.clamp(*day::POINTS_PER_SPAN.start(), *day::POINTS_PER_SPAN.end());
    }

    #[cfg(feature = "screenshot")]
    pub fn open_for_screenshot(&mut self, what: &str) {
        match what.split_once(':') {
            Some(("event", title)) => {
                let Some(place) = self.settings.place else {
                    return;
                };
                let axis = DayAxis::new(date_of(self.selection), place);
                self.detail = self
                    .source
                    .calendars()
                    .occurrences(axis.start(), axis.end())
                    .into_iter()
                    .find(|occurrence| occurrence.title == title);
            }
            _ if what == "settings" => self.settings_open = true,
            _ if what == "moment" => self.start_moment(None),
            _ => {}
        }
    }

    fn demo(&self) -> bool {
        matches!(self.source, Source::Demo(_))
    }

    fn now(&self) -> DateTime<Utc> {
        if self.demo() { demo::now() } else { Utc::now() }
    }

    fn today_at(&self, place: Place) -> i64 {
        bz::bz_date(instant(self.now()), place.longitude).days_since_epoch()
    }

    fn readout(&self, place: Place) -> Readout {
        Readout {
            place,
            scale: self.settings.scale,
            precision: self.settings.precision,
            shows_civil_time: self.settings.shows_civil_time,
            clock: self.clock,
        }
    }

    fn visible(&self) -> impl Fn(&Occurrence) -> bool + '_ {
        |occurrence| {
            !self
                .settings
                .hidden_calendars
                .contains(&occurrence.calendar.0)
        }
    }

    fn moment_calendar(&self, calendars: &[Calendar]) -> Option<Calendar> {
        if self.demo() {
            return None;
        }
        let chosen = self.settings.moment_calendar.as_ref();
        calendars
            .iter()
            .find(|calendar| Some(&calendar.id.0) == chosen)
            .or_else(|| calendars.first())
            .cloned()
    }

    fn go(&mut self, day: i64) {
        self.selection = day;
        self.probe = None;
    }

    fn follow_today(&mut self) {
        if let Some(place) = self.settings.place {
            let today = self.today_at(place);
            if today != self.today {
                if self.selection == self.today {
                    self.selection = today;
                }
                self.today = today;
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context, count: usize) {
        if ctx.input_mut(|i| i.consume_shortcut(&QUIT)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&SETTINGS)) {
            self.settings_open = !self.settings_open;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&REFRESH))
            && let Source::Live(store) = &self.source
        {
            store.refresh();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&MOMENT)) && self.moment.is_none() {
            self.start_moment(None);
        }
        let modal = self.moment.is_some() || self.detail.is_some() || self.all_day.is_some();
        if modal || ctx.memory(|memory| memory.focused().is_some()) {
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.detail = None;
                self.all_day = None;
            }
            return;
        }
        let step = count as i64;
        if ctx.input(|i| i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::PageUp)) {
            self.go(self.selection - step);
        }
        if ctx.input(|i| i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::PageDown)) {
            self.go(self.selection + step);
        }
        if ctx.input(|i| i.key_pressed(Key::T) || i.key_pressed(Key::Home)) {
            self.go(self.today);
        }
    }

    fn page_start(&self, count: usize) -> i64 {
        if count == 7 {
            day_of(week_start(
                naive(date_of(self.selection)),
                self.first_weekday,
            ))
        } else {
            self.selection
        }
    }

    fn title(&self, start: i64, count: usize) -> String {
        let first = naive(date_of(start));
        if count == 1 {
            return first.format("%A, %-d %B %Y").to_string();
        }
        let last = naive(date_of(start + count as i64 - 1));
        if first.format("%m%Y").to_string() == last.format("%m%Y").to_string() {
            format!("{} – {}", first.format("%-d"), last.format("%-d %B %Y"))
        } else {
            format!("{} – {}", first.format("%-d %b"), last.format("%-d %b %Y"))
        }
    }

    fn toolbar(&mut self, ui: &mut Ui, start: i64, count: usize, calendars: &[Calendar]) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if ui
                .add_enabled(self.selection != self.today, egui::Button::new("Today"))
                .clicked()
            {
                self.go(self.today);
            }
            let step = count as i64;
            if ui.button("‹").on_hover_text("Earlier").clicked() {
                self.go(self.selection - step);
            }
            if ui.button("›").on_hover_text("Later").clicked() {
                self.go(self.selection + step);
            }
            let shown = date_of(self.selection);
            let mut picked =
                jiff::civil::date(shown.year as i16, shown.month as i8, shown.day as i8);
            let before = picked;
            ui.add(
                egui_extras::DatePickerButton::new(&mut picked)
                    .id_salt("go to date")
                    .format("%-d %b %Y")
                    .calendar_week(false),
            );
            if picked != before
                && let Some(date) = NaiveDate::from_ymd_opt(
                    i32::from(picked.year()),
                    picked.month() as u32,
                    picked.day() as u32,
                )
            {
                self.go(day_of(date));
            }
            ui.label(
                RichText::new(self.title(start, count))
                    .size(16.0)
                    .color(palette::LINE)
                    .strong(),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("⚙").on_hover_text("Settings").clicked() {
                    self.settings_open = !self.settings_open;
                }
                let can_set = self.moment_calendar(calendars).is_some() || self.demo();
                if ui
                    .add_enabled(can_set, egui::Button::new("Set a Moment"))
                    .clicked()
                {
                    self.start_moment(None);
                }
                if let Source::Live(store) = &self.source {
                    match store.status() {
                        Status::Syncing => {
                            ui.spinner();
                        }
                        Status::Failed(error) => {
                            ui.label(RichText::new("⚠").color(palette::SUN))
                                .on_hover_text(error);
                        }
                        Status::Synced(_) | Status::NoAccounts => {}
                    }
                }
            });
        });
    }

    fn start_moment(&mut self, at: Option<(i64, DateTime<Utc>)>) {
        let Some(place) = self.settings.place else {
            return;
        };
        let readout = self.readout(place);
        let (day, at) = at.unwrap_or_else(|| {
            let now = self.now();
            let axis = DayAxis::new(date_of(self.selection), place);
            (
                self.selection,
                if axis.contains(now) { now } else { axis.noon() },
            )
        });
        self.moment = Some(Draft::new(at, DayAxis::new(date_of(day), place), &readout));
    }

    fn header(&mut self, ui: &mut Ui, columns: &[Column], readout: &Readout, colors: &Colors) {
        let single = columns.len() == 1;
        if single {
            if columns[0].all_day.is_empty() {
                return;
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                if let Some(picked) = allday::strip(ui, columns[0].day, &columns[0].all_day, colors)
                {
                    self.pick(picked);
                }
            });
            ui.add_space(6.0);
            return;
        }
        let lane = (ui.available_width() - day::GUTTER) / columns.len() as f32;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(day::GUTTER);
            for column in columns {
                ui.allocate_ui_with_layout(
                    egui::vec2(lane, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_width(lane - HEADER_MARGIN);
                        ui.add_space(6.0);
                        let today = column.day == self.today;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(column.date.format("%a").to_string())
                                    .size(13.0)
                                    .color(if today { palette::SUN } else { palette::LABEL }),
                            );
                            ui.label(
                                RichText::new(column.date.format("%-d").to_string())
                                    .size(18.0)
                                    .strong()
                                    .color(if today { palette::SUN } else { palette::LINE }),
                            );
                        });
                        if readout.shows_civil_time
                            && let (Some(sunrise), Some(sunset)) =
                                (column.axis.sunrise(), column.axis.sunset())
                        {
                            ui.horizontal(|ui| {
                                ui.add_space(6.0);
                                for (glyph, at) in [
                                    (crate::display::Glyph::Sunrise, sunrise),
                                    (crate::display::Glyph::Sunset, sunset),
                                ] {
                                    Label::new(glyph, None, false, 10.0, palette::LABEL).show(ui);
                                    ui.label(
                                        RichText::new(readout.clock.time(at))
                                            .size(10.0)
                                            .color(palette::LABEL),
                                    );
                                    ui.add_space(8.0);
                                }
                            });
                        }
                        ui.horizontal(|ui| {
                            ui.add_space(6.0);
                            if let Some(picked) =
                                allday::strip(ui, column.day, &column.all_day, colors)
                            {
                                self.pick(picked);
                            }
                        });
                        ui.add_space(6.0);
                    },
                );
                ui.add_space(HEADER_MARGIN);
            }
        });
    }

    fn pick(&mut self, picked: Picked) {
        match picked {
            Picked::One(occurrence) => self.detail = Some(occurrence),
            Picked::All(list) => self.all_day = Some(list),
        }
    }

    fn opening_offset(&self, columns: &[Column], points_per_span: f32) -> f32 {
        let now = self.now();
        let target = match columns.iter().find(|column| column.day == self.today) {
            Some(column) => column.axis.position(now),
            None => columns
                .iter()
                .filter_map(Column::first_position)
                .min_by(f64::total_cmp)
                .unwrap_or(100.0),
        };
        (target as f32 * points_per_span - OPENING_MARGIN).max(0.0)
    }

    fn zoom(&mut self, ctx: &egui::Context, points_per_span: f32) -> f32 {
        let (zoom, pointer) = ctx.input(|i| (i.zoom_delta(), i.pointer.hover_pos()));
        let keys = ctx.input_mut(|i| {
            let zoom_in = i.consume_key(Modifiers::COMMAND, Key::Plus)
                || i.consume_key(Modifiers::COMMAND, Key::Equals);
            let zoom_out = i.consume_key(Modifiers::COMMAND, Key::Minus);
            if zoom_in {
                1.25
            } else if zoom_out {
                0.8
            } else {
                1.0
            }
        });
        let factor = if pointer.is_some_and(|at| self.day_rect.contains(at)) {
            zoom * keys
        } else {
            keys
        };
        if (factor - 1.0).abs() < f32::EPSILON {
            return points_per_span;
        }
        let fitting = day::fitting_scale(self.visible_height);
        let zoomed = (points_per_span * factor).clamp(fitting, *day::POINTS_PER_SPAN.end());
        let anchor = pointer
            .filter(|at| self.day_rect.contains(*at))
            .map_or(self.visible_height / 2.0, |at| at.y - self.day_rect.top());
        let span = (self.scroll_offset + anchor) / points_per_span;
        self.scroll_to = Some((span * zoomed - anchor).max(0.0));
        self.settings.points_per_span = zoomed;
        zoomed
    }

    fn days(&mut self, ui: &mut Ui, place: Place, calendars: &[Calendar]) {
        let readout = self.readout(place);
        let colors = Colors(
            calendars
                .iter()
                .map(|calendar| (calendar.id.clone(), calendar.color))
                .collect::<HashMap<_, _>>(),
        );
        let count = day::day_count(ui.available_width());
        self.shortcuts(ui.ctx(), count);
        let start = self.page_start(count);
        let columns: Vec<Column> = (start..start + count as i64)
            .map(|day| Column::new(day, place, self.source.calendars(), &self.visible()))
            .collect();

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(4.0);
            self.toolbar(ui, start, count, calendars);
            ui.add_space(4.0);
        });
        egui::Panel::bottom("week")
            .frame(
                egui::Frame::side_top_panel(&ui.ctx().global_style())
                    .inner_margin(egui::Margin::symmetric(0, 2)),
            )
            .show(ui, |ui| {
                if let Some(day) = week::strip(ui, self.selection, self.today, self.first_weekday) {
                    self.go(day);
                }
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(palette::NIGHT))
            .show(ui, |ui| {
                self.header(ui, &columns, &readout, &colors);
                let fitting = day::fitting_scale(self.visible_height);
                let mut points_per_span = self
                    .settings
                    .points_per_span
                    .clamp(fitting, *day::POINTS_PER_SPAN.end());
                points_per_span = self.zoom(ui.ctx(), points_per_span);
                if self.page != Some((start, count)) {
                    self.page = Some((start, count));
                    self.scroll_to = Some(self.opening_offset(&columns, points_per_span));
                }
                let mut area = ScrollArea::vertical()
                    .id_salt("day")
                    .auto_shrink([false, false]);
                if let Some(offset) = self.scroll_to.take() {
                    area = area.vertical_scroll_offset(offset);
                }
                let now = self.now();
                let view = DayView {
                    columns: &columns,
                    colors: &colors,
                    readout,
                    points_per_span,
                    now,
                };
                let output = area.show(ui, |ui| view.show(ui, &mut self.probe));
                self.scroll_offset = output.state.offset.y;
                self.visible_height = output.inner_rect.height();
                self.day_rect = output.inner_rect;
                match output.inner {
                    Some(Action::Open(occurrence)) => self.detail = Some(occurrence),
                    Some(Action::SetMoment { day, at }) => self.start_moment(Some((day, at))),
                    None => {}
                }
            });
        ui.ctx().request_repaint_after(Duration::from_secs(15));
    }

    fn windows(&mut self, ctx: &egui::Context, place: Option<Place>, calendars: &[Calendar]) {
        let Some(place) = place else {
            return;
        };
        let readout = self.readout(place);
        if let Some(occurrence) = self.detail.clone() {
            let mut open = true;
            let calendar = calendars
                .iter()
                .find(|calendar| calendar.id == occurrence.calendar);
            egui::Window::new("Event")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(380.0)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| detail::show(ui, &occurrence, calendar, &readout));
            if !open {
                self.detail = None;
            }
        }
        if let Some(list) = self.all_day.clone() {
            let mut open = true;
            let mut chosen = None;
            egui::Window::new("All day")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    for occurrence in &list {
                        if ui.selectable_label(false, &occurrence.title).clicked() {
                            chosen = Some(occurrence.clone());
                        }
                    }
                });
            if chosen.is_some() || !open {
                self.all_day = None;
                self.detail = chosen;
            }
        }
        if self.moment.is_some() {
            let calendar = self.moment_calendar(calendars);
            let draft = self.moment.as_mut().expect("checked above");
            let response = egui::Modal::new(egui::Id::new("moment"))
                .show(ctx, |ui| draft.show(ui, &readout, calendar.as_ref()));
            match response.inner {
                Outcome::Add { at, title } => {
                    if let Some(calendar) = calendar {
                        let reply = self.source.calendars().add_moment(at, title, &calendar.id);
                        self.moment.as_mut().expect("checked above").wait(reply);
                    }
                }
                Outcome::Added | Outcome::Cancelled => self.moment = None,
                Outcome::Open if response.should_close() => self.moment = None,
                Outcome::Open => {}
            }
        }
    }

    fn settings_window(&mut self, ctx: &egui::Context, calendars: &[Calendar]) {
        let mut open = self.settings_open;
        egui::Window::new("Settings")
            .open(&mut open)
            .collapsible(false)
            .default_width(440.0)
            .default_height(560.0)
            .vscroll(true)
            .show(ctx, |ui| self.settings_content(ui, calendars));
        self.settings_open = open;
    }

    fn settings_content(&mut self, ui: &mut Ui, calendars: &[Calendar]) {
        section(ui, "Bogenzeit");
        egui::Grid::new("bogenzeit")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                ui.label("Scale");
                egui::ComboBox::from_id_salt("scale")
                    .selected_text(self.settings.scale.title())
                    .show_ui(ui, |ui| {
                        for scale in Scale::ALL {
                            ui.selectable_value(&mut self.settings.scale, scale, scale.title());
                        }
                    });
                ui.end_row();
                ui.label("Precision");
                egui::ComboBox::from_id_salt("precision")
                    .selected_text(self.settings.precision.title())
                    .show_ui(ui, |ui| {
                        for precision in Precision::ALL {
                            ui.selectable_value(
                                &mut self.settings.precision,
                                precision,
                                precision.title(),
                            );
                        }
                    });
                ui.end_row();
            });
        ui.checkbox(&mut self.settings.shows_civil_time, "Show civil time");
        note(
            ui,
            "Event times read as spans or kiloseconds before or after solar noon or solar midnight where you are. Civil time is shown beside them, for the people who keep the clock.",
        );

        let mut accounts: Vec<&str> = calendars
            .iter()
            .map(|calendar| calendar.account.as_str())
            .collect();
        accounts.dedup();
        for account in accounts {
            section(ui, &account_title(account));
            for calendar in calendars
                .iter()
                .filter(|calendar| calendar.account == account)
            {
                let mut shown = !self.settings.hidden_calendars.contains(&calendar.id.0);
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut shown, "").changed() {
                        if shown {
                            self.settings.hidden_calendars.remove(&calendar.id.0);
                        } else {
                            self.settings.hidden_calendars.insert(calendar.id.0.clone());
                        }
                    }
                    let (dot, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter().circle_filled(
                        dot.center(),
                        5.0,
                        palette::calendar(calendar.color),
                    );
                    ui.label(&calendar.title);
                });
            }
        }

        if let Source::Live(store) = &self.source {
            section(ui, "Moments");
            let chosen = self.moment_calendar(calendars);
            egui::ComboBox::from_id_salt("moment calendar")
                .selected_text(
                    chosen
                        .as_ref()
                        .map_or("None".to_string(), |calendar| calendar.title.clone()),
                )
                .show_ui(ui, |ui| {
                    for calendar in calendars {
                        let selected = chosen
                            .as_ref()
                            .is_some_and(|chosen| chosen.id == calendar.id);
                        if ui.selectable_label(selected, &calendar.title).clicked() {
                            self.settings.moment_calendar = Some(calendar.id.0.clone());
                        }
                    }
                });
            note(
                ui,
                "The calendar a moment is added to. Its alert is the calendar's own, so it reaches every device that has the calendar.",
            );

            section(ui, "Accounts");
            for account in store.accounts() {
                ui.horizontal(|ui| {
                    ui.label(format!("{} on {}", account.username, account.server));
                    if ui.small_button("Sign Out").clicked() {
                        store.sign_out(account.clone());
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(status_text(&store.status()))
                        .size(11.0)
                        .color(palette::LABEL),
                );
                if ui.small_button("Sync Now").clicked() {
                    store.refresh();
                }
            });
            ui.collapsing("Add an account", |ui| self.sign_in.show(ui, store));
            note(
                ui,
                "Passwords are kept in the GNOME keyring. Events are cached on this computer and sent nowhere but the calendar server.",
            );
        }

        section(ui, "Place");
        if !self.demo() {
            self.coordinates(ui);
        } else if let Some(place) = self.settings.place {
            ui.label(format!("{:.2}, {:.2}", place.latitude, place.longitude));
        }

        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                    .size(11.0)
                    .monospace()
                    .color(palette::LABEL),
            );
        });
    }

    fn coordinates(&mut self, ui: &mut Ui) {
        egui::Grid::new("coordinates")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                ui.label("Latitude");
                ui.add(egui::TextEdit::singleline(&mut self.latitude).desired_width(120.0));
                ui.end_row();
                ui.label("Longitude");
                ui.add(egui::TextEdit::singleline(&mut self.longitude).desired_width(120.0));
                ui.end_row();
            });
        let parsed = (
            self.latitude.trim().parse::<f64>(),
            self.longitude.trim().parse::<f64>(),
        );
        let valid = matches!(parsed, (Ok(lat), Ok(lon)) if (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon));
        if ui
            .add_enabled(valid, egui::Button::new("Use These"))
            .clicked()
            && let (Ok(latitude), Ok(longitude)) = parsed
        {
            let first = self.settings.place.is_none();
            self.settings.place = Some(Place {
                latitude,
                longitude,
            });
            if first {
                self.today = self.today_at(Place {
                    latitude,
                    longitude,
                });
                self.selection = self.today;
            }
        }
        note(
            ui,
            "Degrees, north and east positive. Kept on this computer.",
        );
    }

    fn gate(
        &mut self,
        ui: &mut Ui,
        title: &str,
        message: &str,
        content: impl FnOnce(&mut App, &mut Ui),
    ) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(palette::NIGHT))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space((ui.available_height() / 2.0 - 180.0).max(24.0));
                    ui.label(
                        RichText::new(title)
                            .size(22.0)
                            .strong()
                            .color(palette::LINE),
                    );
                    ui.add_space(12.0);
                    ui.set_max_width(440.0);
                    ui.label(RichText::new(message).color(palette::LABEL));
                    ui.add_space(16.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width().min(440.0), 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| content(self, ui),
                    );
                });
            });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.follow_today();
        let calendars = self.source.calendars().calendars();
        let before = self.settings.clone();

        let needs_account = match &self.source {
            Source::Live(store) => {
                calendars.is_empty() && matches!(store.status(), Status::NoAccounts)
            }
            Source::Demo(_) => false,
        };
        if needs_account {
            self.gate(
                ui,
                "Your calendars",
                "Bogenplan shows your calendars laid out on the Sun's arc, and adds the moments you set to them. Sign in to iCloud or another CalDAV server.",
                |app, ui| {
                    if let Source::Live(store) = &app.source {
                        app.sign_in.show(ui, store);
                    }
                },
            );
        } else if let Some(place) = self.settings.place {
            self.days(ui, place, &calendars);
        } else {
            self.gate(
                ui,
                "Where you are",
                "Solar noon, sunrise and sunset depend on where you are.",
                |app, ui| app.coordinates(ui),
            );
        }
        self.windows(&ctx, self.settings.place, &calendars);
        self.settings_window(&ctx, &calendars);
        if self.settings != before && !self.demo() {
            self.settings.save();
        }
        #[cfg(feature = "screenshot")]
        if let Some(screenshot) = &mut self.screenshot {
            screenshot.tick(&ctx);
        }
    }
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(title).size(12.0).strong().color(palette::SUN));
    ui.add_space(2.0);
}

fn note(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(palette::LABEL));
}

fn account_title(key: &str) -> String {
    match key.rsplit_once('@') {
        Some((username, crate::accounts::ICLOUD)) => format!("iCloud · {username}"),
        Some((username, server)) => format!("{server} · {username}"),
        None => key.to_string(),
    }
}

fn status_text(status: &Status) -> String {
    match status {
        Status::Syncing => "Syncing".to_string(),
        Status::Synced(at) => format!(
            "Synced {}",
            at.with_timezone(&chrono::Local).format("%-d %b, %H:%M")
        ),
        Status::NoAccounts => "No accounts".to_string(),
        Status::Failed(error) => format!("Last sync failed: {error}"),
    }
}

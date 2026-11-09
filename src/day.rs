// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::ops::RangeInclusive;

use bogenplan_store::{CalendarSource, Occurrence, OccurrenceTime};
use chrono::{DateTime, NaiveDate, Utc};
use eframe::egui::text::{LayoutJob, TextWrapping};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Sense, Stroke, TextFormat,
    Ui, pos2, vec2,
};

use crate::axis::{DayAxis, LENGTH, Mark};
use crate::civil::{civil_interval, date_of, local_midnight, naive};
use crate::display::Readout;
use crate::glyph::Label;
use crate::layout;
use crate::palette::{self, with_opacity};
use crate::place::Place;

pub struct Colors(pub std::collections::HashMap<bogenplan_store::CalendarId, [u8; 3]>);

impl Colors {
    pub fn of(&self, occurrence: &Occurrence) -> Color32 {
        palette::calendar(
            self.0
                .get(&occurrence.calendar)
                .copied()
                .unwrap_or([0x8F, 0xA3, 0xBF]),
        )
    }
}

pub const GUTTER: f32 = 72.0;
pub const FOOTER: f32 = 40.0;
pub const POINTS_PER_SPAN: RangeInclusive<f32> = 1.5..=16.0;
const MARK_STEPS: [(usize, f32); 2] = [(10, 3.2), (25, 1.8)];
const LANE_WIDTH: f32 = 200.0;
const WEEK_LANE_WIDTH: f32 = 150.0;
const MINIMUM_EVENT_POINTS: f32 = 24.0;
const HOLD_TO_SHOW: f64 = 0.25;
const HOLD_TO_ARM: f64 = 0.5;
const STILL_POINTS: f32 = 6.0;

pub struct Column {
    pub day: i64,
    pub date: NaiveDate,
    pub axis: DayAxis,
    pub timed: Vec<Occurrence>,
    pub all_day: Vec<Occurrence>,
}

impl Column {
    pub fn new(
        day: i64,
        place: Place,
        source: &dyn CalendarSource,
        visible: &dyn Fn(&Occurrence) -> bool,
    ) -> Column {
        let bz_date = date_of(day);
        let date = naive(bz_date);
        let axis = DayAxis::new(bz_date, place);
        let (civil_start, civil_end) = civil_interval(date);
        let civil_noon = civil_start + (civil_end - civil_start) / 2;
        let events: Vec<Occurrence> = source
            .occurrences(axis.start().min(civil_start), axis.end().max(civil_end))
            .into_iter()
            .filter(|occurrence| visible(occurrence))
            .collect();
        let mut all_day: Vec<Occurrence> = events
            .iter()
            .filter(|occurrence| match occurrence.time {
                OccurrenceTime::AllDay { start, end } => {
                    local_midnight(start) <= civil_noon && civil_noon < local_midnight(end)
                }
                OccurrenceTime::Timed { .. } => false,
            })
            .cloned()
            .collect();
        all_day.sort_by(|a, b| a.title.cmp(&b.title));
        let timed = events
            .into_iter()
            .filter(|occurrence| match occurrence.time {
                OccurrenceTime::Timed { start, end } => {
                    (end > axis.start() || (start == end && start >= axis.start()))
                        && start < axis.end()
                }
                OccurrenceTime::AllDay { .. } => false,
            })
            .collect();
        Column {
            day,
            date,
            axis,
            timed,
            all_day,
        }
    }

    pub fn first_position(&self) -> Option<f64> {
        self.timed
            .iter()
            .filter_map(|occurrence| match occurrence.time {
                OccurrenceTime::Timed { start, .. } if self.axis.contains(start) => {
                    Some(self.axis.position(start))
                }
                _ => None,
            })
            .min_by(f64::total_cmp)
    }
}

pub fn day_count(width: f32) -> usize {
    let lanes = width - GUTTER;
    if lanes / 7.0 >= WEEK_LANE_WIDTH {
        return 7;
    }
    ((lanes / LANE_WIDTH) as usize).clamp(1, 4)
}

pub fn height(points_per_span: f32) -> f32 {
    LENGTH as f32 * points_per_span + FOOTER
}

pub fn fitting_scale(visible_height: f32) -> f32 {
    ((visible_height - FOOTER) / LENGTH as f32).max(*POINTS_PER_SPAN.start())
}

fn marks(axis: &DayAxis, points_per_span: f32) -> Vec<Mark> {
    let step = MARK_STEPS
        .iter()
        .find(|(_, minimum)| points_per_span >= *minimum)
        .map_or(50, |(step, _)| *step);
    axis.marks(step)
}

#[derive(Debug, Clone, Copy)]
pub struct Probe {
    pub day: i64,
    pub y: f32,
    pressed_at: f64,
    still_at: f64,
    still: Pos2,
    shown: bool,
    pub armed: bool,
}

pub enum Action {
    Open(Occurrence),
    SetMoment { day: i64, at: DateTime<Utc> },
}

pub struct DayView<'a> {
    pub columns: &'a [Column],
    pub colors: &'a Colors,
    pub readout: Readout,
    pub points_per_span: f32,
    pub now: DateTime<Utc>,
}

impl DayView<'_> {
    fn y(&self, top: f32, position: f64) -> f32 {
        top + position as f32 * self.points_per_span
    }

    pub fn show(&self, ui: &mut Ui, probe: &mut Option<Probe>) -> Option<Action> {
        let width = ui.available_width();
        let (rect, _) =
            ui.allocate_exact_size(vec2(width, height(self.points_per_span)), Sense::hover());
        let painter = ui.painter_at(rect);
        let single = self.columns.len() == 1;
        let gutter = Rect::from_min_size(rect.min, vec2(GUTTER, rect.height()));
        self.sky(&painter, gutter, &self.columns[0].axis);
        self.gutter_labels(
            &painter,
            gutter,
            &self.columns[0].axis,
            single && self.readout.shows_civil_time,
        );

        let lane_width = (width - GUTTER) / self.columns.len() as f32;
        let mut action = None;
        for (i, column) in self.columns.iter().enumerate() {
            let lane = Rect::from_min_size(
                pos2(rect.left() + GUTTER + i as f32 * lane_width, rect.top()),
                vec2(lane_width, rect.height()),
            );
            self.sky(&painter, lane, &column.axis);
            if !single {
                painter.vline(
                    lane.left(),
                    lane.y_range(),
                    Stroke::new(0.5, with_opacity(palette::MAJOR_TICK, 0.25)),
                );
            }
            let blocks = self.events(&painter, lane, column);
            self.now_line(&painter, lane, &column.axis);
            if let Some(found) = self.interact(ui, lane, column, &blocks, probe) {
                action = Some(found);
            }
            if let Some(shown) = probe.filter(|probe| probe.day == column.day && probe.shown) {
                self.probe_line(&painter, lane, column, &shown);
            }
        }
        action
    }

    fn sky(&self, painter: &Painter, rect: Rect, axis: &DayAxis) {
        painter.rect_filled(rect, CornerRadius::ZERO, palette::NIGHT);
        let bands = [
            (axis.solar.astronomical, palette::ASTRONOMICAL_TWILIGHT),
            (axis.solar.nautical, palette::NAUTICAL_TWILIGHT),
            (axis.solar.civil, palette::CIVIL_TWILIGHT),
            (axis.solar.horizon, palette::DAY),
        ];
        for (arc, color) in bands {
            if let Some((from, to)) = axis.extent(arc) {
                let band = Rect::from_x_y_ranges(
                    rect.x_range(),
                    self.y(rect.top(), from)..=self.y(rect.top(), to),
                );
                painter.rect_filled(band, CornerRadius::ZERO, color);
            }
        }
        for mark in marks(axis, self.points_per_span) {
            let opacity = if mark.number.is_none() {
                0.45
            } else if mark.eighth_boundary {
                0.2
            } else {
                0.08
            };
            painter.hline(
                rect.x_range(),
                self.y(rect.top(), mark.position),
                Stroke::new(0.5, with_opacity(palette::MAJOR_TICK, opacity)),
            );
        }
    }

    fn gutter_labels(&self, painter: &Painter, rect: Rect, axis: &DayAxis, civil: bool) {
        for mark in marks(axis, self.points_per_span) {
            let y = self.y(rect.top(), mark.position) + 3.0;
            let (size, color) = if mark.eighth_boundary {
                (11.0, palette::LINE)
            } else {
                (10.0, with_opacity(palette::LABEL, 0.7))
            };
            let label = Label::new(mark.glyph, mark.number.clone(), mark.before, size, color);
            let drawn = label.paint(
                painter,
                pos2(rect.left() + 8.0, y + label.size(painter).y / 2.0),
            );
            if mark.eighth_boundary && civil {
                painter.text(
                    pos2(rect.left() + 8.0, drawn.bottom() + 1.0),
                    Align2::LEFT_TOP,
                    self.readout.clock.time(mark.instant),
                    FontId::proportional(10.0),
                    palette::LABEL,
                );
            }
        }
    }

    fn events(&self, painter: &Painter, lane: Rect, column: &Column) -> Vec<(Rect, usize)> {
        let spans: Vec<(f64, f64)> = column
            .timed
            .iter()
            .map(|occurrence| {
                let (start, end) = occurrence.time.utc_bounds();
                (column.axis.position(start), column.axis.position(end))
            })
            .collect();
        let minimum = f64::from(MINIMUM_EVENT_POINTS / self.points_per_span);
        let width = lane.width() - 8.0;
        let mut blocks = Vec::new();
        for placed in layout::place(&spans, minimum, LENGTH) {
            let column_width = width / placed.columns as f32;
            let block = Rect::from_min_max(
                pos2(
                    lane.left() + placed.column as f32 * column_width,
                    self.y(lane.top(), placed.top),
                ),
                pos2(
                    lane.left() + (placed.column + 1) as f32 * column_width - 2.0,
                    self.y(lane.top(), placed.bottom) - 1.0,
                ),
            );
            self.event_block(painter, block, &column.timed[placed.index]);
            blocks.push((block, placed.index));
        }
        blocks
    }

    fn event_block(&self, painter: &Painter, block: Rect, occurrence: &Occurrence) {
        let OccurrenceTime::Timed { start, end } = occurrence.time else {
            return;
        };
        let color = self.colors.of(occurrence);
        let rounding = CornerRadius::same(4);
        painter.rect_filled(block, rounding, with_opacity(palette::NIGHT, 0.7));
        painter.rect_filled(block, rounding, with_opacity(color, 0.3));
        painter.rect_filled(
            Rect::from_min_size(block.min, vec2(3.0, block.height())),
            CornerRadius {
                nw: 4,
                sw: 4,
                ne: 0,
                se: 0,
            },
            color,
        );
        let painter = painter.with_clip_rect(block.shrink(1.0));
        let inner = Rect::from_min_max(block.min + vec2(8.0, 3.0), block.max - vec2(4.0, 3.0));
        let start_label = Label::of(&self.readout.position(start), 10.0, palette::LINE);
        let end_label = Label::of(&self.readout.position(end), 10.0, palette::LINE);
        let civil_text = format!(
            "{} – {}",
            self.readout.clock.time(start),
            self.readout.clock.time(end)
        );
        let rows = |title_rows: f32, range: bool, civil: bool| {
            14.0 * title_rows + if range { 15.0 } else { 0.0 } + if civil { 14.0 } else { 0.0 }
        };
        let (title_rows, range, civil) =
            if rows(2.0, true, self.readout.shows_civil_time) <= inner.height() {
                (2, true, self.readout.shows_civil_time)
            } else if rows(1.0, true, false) <= inner.height() {
                (
                    if rows(2.0, true, false) <= inner.height() {
                        2
                    } else {
                        1
                    },
                    true,
                    false,
                )
            } else {
                (1, false, false)
            };
        let title = text(
            &painter,
            &occurrence.title,
            11.0,
            palette::LINE,
            inner.width(),
            title_rows,
        );
        let mut y = inner.top();
        let title_height = title.size().y;
        painter.galley(pos2(inner.left(), y), title, palette::LINE);
        y += title_height + 2.0;
        if range {
            let center = y + 6.0;
            let both = start_label.size(&painter).x + end_label.size(&painter).x + 16.0;
            let drawn = start_label.paint(&painter, pos2(inner.left(), center));
            if both <= inner.width() && start != end {
                let dash = painter.text(
                    pos2(drawn.right() + 4.0, center),
                    Align2::LEFT_CENTER,
                    "–",
                    FontId::proportional(10.0),
                    palette::LINE,
                );
                end_label.paint(&painter, pos2(dash.right() + 4.0, center));
            }
            y += 15.0;
        }
        if civil {
            painter.text(
                pos2(inner.left(), y),
                Align2::LEFT_TOP,
                civil_text,
                FontId::proportional(10.0),
                palette::LABEL,
            );
        }
    }

    fn now_line(&self, painter: &Painter, lane: Rect, axis: &DayAxis) {
        if !axis.contains(self.now) {
            return;
        }
        let y = self.y(lane.top(), axis.position(self.now));
        painter.circle_filled(pos2(lane.left(), y), 4.0, palette::SUN);
        painter.hline(lane.x_range(), y, Stroke::new(1.5, palette::SUN));
    }

    fn interact(
        &self,
        ui: &Ui,
        lane: Rect,
        column: &Column,
        blocks: &[(Rect, usize)],
        probe: &mut Option<Probe>,
    ) -> Option<Action> {
        let response = ui.interact(lane, ui.id().with(("lane", column.day)), Sense::click());
        let (pointer, time, pressed, down, released) = ui.input(|input| {
            (
                input.pointer.interact_pos(),
                input.time,
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
                input.pointer.primary_released(),
            )
        });
        let over_event = |at: Pos2| {
            blocks
                .iter()
                .find(|(block, _)| block.contains(at))
                .map(|(_, index)| *index)
        };
        let instant_at = |y: f32| {
            column
                .axis
                .instant_at(f64::from((y - lane.top()) / self.points_per_span))
        };

        if response.clicked()
            && let Some(index) = pointer.and_then(over_event)
        {
            *probe = None;
            return Some(Action::Open(column.timed[index].clone()));
        }
        if response.double_clicked()
            && let Some(at) = pointer.filter(|at| over_event(*at).is_none())
        {
            *probe = None;
            return Some(Action::SetMoment {
                day: column.day,
                at: instant_at(at.y),
            });
        }
        let at = pointer?;
        if pressed && lane.contains(at) && over_event(at).is_none() && response.hovered() {
            *probe = Some(Probe {
                day: column.day,
                y: at.y,
                pressed_at: time,
                still_at: time,
                still: at,
                shown: false,
                armed: false,
            });
        }
        let current = probe.as_mut().filter(|probe| probe.day == column.day)?;
        if released || !down {
            let set = current.armed.then(|| Action::SetMoment {
                day: column.day,
                at: instant_at(current.y),
            });
            *probe = None;
            return set;
        }
        current.y = at.y.clamp(
            lane.top(),
            lane.top() + LENGTH as f32 * self.points_per_span,
        );
        if (at - current.still).length() > STILL_POINTS {
            current.still = at;
            current.still_at = time;
            current.armed = false;
        }
        if !current.shown && time - current.pressed_at >= HOLD_TO_SHOW {
            current.shown = true;
            current.still_at = time;
        } else if current.shown && time - current.still_at >= HOLD_TO_ARM {
            current.armed = true;
        }
        ui.ctx().request_repaint();
        None
    }

    fn probe_line(&self, painter: &Painter, lane: Rect, column: &Column, probe: &Probe) {
        let color = if probe.armed {
            palette::SUN
        } else {
            palette::LINE
        };
        let painter = painter.with_clip_rect(painter.clip_rect().expand(200.0));
        painter.hline(lane.x_range(), probe.y, Stroke::new(1.5, color));
        let at = column
            .axis
            .instant_at(f64::from((probe.y - lane.top()) / self.points_per_span));
        let label = Label::of(&self.readout.position(at), 14.0, color);
        let mut width = label.size(&painter).x + 20.0;
        let civil = self
            .readout
            .shows_civil_time
            .then(|| self.readout.clock.time(at));
        let civil_width = civil.as_ref().map_or(0.0, |civil| {
            painter
                .layout_no_wrap(civil.clone(), FontId::proportional(14.0), palette::LABEL)
                .size()
                .x
                + 8.0
        });
        let armed_width = if probe.armed { 104.0 } else { 0.0 };
        width += civil_width + armed_width;
        let capsule = Rect::from_min_size(
            pos2(lane.left() + 8.0, (probe.y - 44.0).max(lane.top())),
            vec2(width, 30.0),
        );
        painter.rect_filled(
            capsule,
            CornerRadius::same(15),
            with_opacity(palette::NIGHT, 0.92),
        );
        painter.rect_stroke(
            capsule,
            CornerRadius::same(15),
            Stroke::new(0.5, with_opacity(color, 0.4)),
            egui::StrokeKind::Inside,
        );
        let drawn = label.paint(&painter, pos2(capsule.left() + 10.0, capsule.center().y));
        let mut x = drawn.right() + 8.0;
        if let Some(civil) = civil {
            x = painter
                .text(
                    pos2(x, capsule.center().y),
                    Align2::LEFT_CENTER,
                    civil,
                    FontId::proportional(14.0),
                    palette::LABEL,
                )
                .right()
                + 8.0;
        }
        if probe.armed {
            painter.text(
                pos2(x, capsule.center().y),
                Align2::LEFT_CENTER,
                "Release to set",
                FontId::proportional(12.0),
                palette::SUN,
            );
        }
    }
}

pub fn text(
    painter: &Painter,
    text: &str,
    size: f32,
    color: Color32,
    width: f32,
    rows: usize,
) -> std::sync::Arc<egui::Galley> {
    let mut job = LayoutJob::default();
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: FontId::proportional(size),
            color,
            ..Default::default()
        },
    );
    job.wrap = TextWrapping {
        max_width: width.max(1.0),
        max_rows: rows,
        break_anywhere: false,
        overflow_character: Some('…'),
    };
    painter.layout_job(job)
}

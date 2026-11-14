//! Fork: the listening statistics page, from Spotify's streaming history
//! export: totals, hours by month, hours by time of day and weekday, top
//! artists by listening time and top songs by plays.

use egui::{Align2, Rect, Sense, pos2, vec2};

use crate::app::App;
use crate::i18n::gettext;
use crate::model::{Action, Loadable, Page};
use crate::stats::{Stats, hours_text};
use crate::theme::{self, Icon, Palette};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let folder = crate::stats::folder(&crate::paths::AppDirs::discover().dirs);
    ui.add_space(16.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(gettext(app.locale, "Your stats"))
                .font(theme::bold(32.0))
                .color(palette.text),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if theme::soft_button(ui, &palette, Some(Icon::Refresh), "Reload", false).clicked() {
                app.load_stats();
            }
            if theme::soft_button(ui, &palette, Some(Icon::ExternalLink), "Open folder", false)
                .clicked()
            {
                let _ = std::fs::create_dir_all(&folder);
                let _ = crate::opener::open(&folder);
            }
        });
    });
    ui.add_space(12.0);

    match app.stats.clone() {
        Loadable::NotLoaded => app.load_stats(),
        Loadable::Loading => {
            ui.label(
                egui::RichText::new("Reading your streaming history…")
                    .font(theme::regular(14.0))
                    .color(palette.secondary),
            );
        }
        Loadable::Failed(error) => empty(ui, &palette, &folder, &error),
        Loadable::Loaded(stats) => page(app, ui, &palette, &stats),
    }
}

fn empty(ui: &mut egui::Ui, palette: &Palette, folder: &std::path::Path, error: &str) {
    let lines = [
        "Spotify gives no play counts of your own through its API, but its data export does.",
        "1. In your Spotify account, open Privacy settings → Download your data.",
        "2. Tick Extended streaming history and request it. Spotify emails it, which can take up to 30 days.",
        "3. Unzip it and put the Streaming_History_Audio_*.json files in this folder, then press Reload:",
    ];
    for line in lines {
        ui.label(
            egui::RichText::new(line)
                .font(theme::regular(14.0))
                .color(palette.secondary),
        );
        ui.add_space(4.0);
    }
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(folder.display().to_string())
            .font(theme::medium(14.0))
            .color(palette.text),
    );
    if error != "empty" && error != "no_folder" {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(error)
                .font(theme::regular(13.0))
                .color(palette.danger),
        );
    }
}

fn heading(ui: &mut egui::Ui, palette: &Palette, text: &str) {
    ui.add_space(24.0);
    ui.label(
        egui::RichText::new(text)
            .font(theme::bold(20.0))
            .color(palette.text),
    );
    ui.add_space(10.0);
}

fn page(app: &mut App, ui: &mut egui::Ui, palette: &Palette, stats: &Stats) {
    // Totals.
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 32.0;
        for (value, label) in [
            (hours_text(stats.total_ms), "listened"),
            (crate::util::format_count(stats.plays), "plays"),
            (crate::util::format_count(stats.songs as u64), "songs"),
            (crate::util::format_count(stats.artists as u64), "artists"),
        ] {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(value)
                        .font(theme::bold(28.0))
                        .color(palette.accent),
                );
                ui.label(
                    egui::RichText::new(label)
                        .font(theme::regular(13.0))
                        .color(palette.secondary),
                );
            });
        }
    });
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(format!("{} – {}", stats.first, stats.last))
            .font(theme::regular(13.0))
            .color(palette.dim),
    );

    heading(ui, palette, "Hours by month");
    let months: Vec<(String, u64)> = stats.months.iter().rev().take(36).rev().cloned().collect();
    bars(
        ui,
        palette,
        &months
            .iter()
            .map(|(month, ms)| (month[2..].replace('-', "/"), *ms))
            .collect::<Vec<_>>(),
        140.0,
    );

    heading(ui, palette, "When you listen");
    let hours: Vec<(String, u64)> = stats
        .hours
        .iter()
        .enumerate()
        .map(|(hour, ms)| (format!("{hour}"), *ms))
        .collect();
    bars(ui, palette, &hours, 100.0);
    ui.add_space(12.0);
    let days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let weekdays: Vec<(String, u64)> = stats
        .weekdays
        .iter()
        .zip(days)
        .map(|(ms, day)| (day.to_string(), *ms))
        .collect();
    bars(ui, palette, &weekdays, 70.0);

    heading(ui, palette, "Top artists");
    let top = stats
        .top_artists
        .first()
        .map_or(1, |artist| artist.ms.max(1));
    for (rank, artist) in stats.top_artists.iter().enumerate() {
        ranked_row(
            ui,
            palette,
            rank + 1,
            &artist.name,
            None,
            &hours_text(artist.ms),
            artist.ms as f32 / top as f32,
        );
    }

    heading(ui, palette, "Top songs");
    let top = stats.top_songs.first().map_or(1, |song| song.plays.max(1));
    for (rank, song) in stats.top_songs.iter().enumerate() {
        let response = ranked_row(
            ui,
            palette,
            rank + 1,
            &song.name,
            Some(&song.artist),
            &format!("{} plays", crate::util::format_count(u64::from(song.plays))),
            song.plays as f32 / top as f32,
        );
        if response.double_clicked() && !song.uri.is_empty() {
            app.actions.push(Action::PlayUris {
                uris: vec![song.uri.clone()],
                index: 0,
            });
        }
    }
    let _ = Page::Stats;
}

/// A bar chart: one bar per entry, its label under it.
fn bars(ui: &mut egui::Ui, palette: &Palette, entries: &[(String, u64)], height: f32) {
    if entries.is_empty() {
        return;
    }
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, height + 18.0), Sense::hover());
    let max = entries.iter().map(|(_, ms)| *ms).max().unwrap_or(1).max(1);
    let slot = width / entries.len() as f32;
    let gap = (slot * 0.2).clamp(1.0, 6.0);
    let pointer = ui.ctx().pointer_hover_pos();
    // Labels thin out when bars are narrow.
    let every = ((28.0 / slot).ceil() as usize).max(1);
    for (index, (label, ms)) in entries.iter().enumerate() {
        let x = rect.left() + slot * index as f32;
        let bar_height = (height * *ms as f32 / max as f32).max(if *ms > 0 { 2.0 } else { 0.0 });
        let bar = Rect::from_min_max(
            pos2(x + gap / 2.0, rect.top() + height - bar_height),
            pos2(x + slot - gap / 2.0, rect.top() + height),
        );
        let column = Rect::from_min_max(pos2(x, rect.top()), pos2(x + slot, rect.top() + height));
        let hovered = pointer.is_some_and(|p| column.contains(p));
        let color = if hovered {
            palette.accent_hover
        } else {
            palette.accent
        };
        ui.painter().rect_filled(bar, 3.0, color);
        if index % every == 0 {
            ui.painter().text(
                pos2(x + slot / 2.0, rect.top() + height + 4.0),
                Align2::CENTER_TOP,
                label,
                theme::regular(11.0),
                palette.dim,
            );
        }
        if hovered {
            egui::Tooltip::always_open(
                ui.ctx().clone(),
                ui.layer_id(),
                egui::Id::new(("stats-bar", rect.min.x as i32, rect.min.y as i32)),
                egui::PopupAnchor::Pointer,
            )
            .show(|ui| {
                ui.label(format!("{label}: {}", hours_text(*ms)));
            });
        }
    }
}

/// A numbered row with a title, an optional subtitle, a value and a bar
/// showing its share of the top entry.
fn ranked_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    rank: usize,
    title: &str,
    subtitle: Option<&str>,
    value: &str,
    share: f32,
) -> egui::Response {
    let height = if subtitle.is_some() { 48.0 } else { 34.0 };
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 6.0, palette.surface_hover);
    }
    let painter = ui.painter_at(rect);
    painter.text(
        pos2(rect.left() + 28.0, rect.center().y),
        Align2::RIGHT_CENTER,
        rank.to_string(),
        theme::regular(14.0),
        palette.secondary,
    );
    let text_left = rect.left() + 44.0;
    let value_width = 110.0;
    let bar_width = (rect.width() * 0.25).min(220.0);
    let bar_left = rect.right() - value_width - bar_width - 12.0;
    match subtitle {
        Some(subtitle) => {
            painter.text(
                pos2(text_left, rect.center().y - 9.0),
                Align2::LEFT_CENTER,
                title,
                theme::medium(15.0),
                palette.text,
            );
            painter.text(
                pos2(text_left, rect.center().y + 10.0),
                Align2::LEFT_CENTER,
                subtitle,
                theme::regular(13.0),
                palette.secondary,
            );
        }
        None => {
            painter.text(
                pos2(text_left, rect.center().y),
                Align2::LEFT_CENTER,
                title,
                theme::medium(15.0),
                palette.text,
            );
        }
    }
    let track = Rect::from_min_size(pos2(bar_left, rect.center().y - 3.0), vec2(bar_width, 6.0));
    painter.rect_filled(track, 3.0, palette.surface);
    let filled = Rect::from_min_size(track.min, vec2(bar_width * share.clamp(0.0, 1.0), 6.0));
    painter.rect_filled(filled, 3.0, palette.accent);
    painter.text(
        pos2(rect.right() - 8.0, rect.center().y),
        Align2::RIGHT_CENTER,
        value,
        theme::regular(13.0),
        palette.secondary,
    );
    response
}

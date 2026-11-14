//! Fork: the Now playing panel on the right, as Spotify's desktop has it: the
//! cover, the song with its save mark, the artists, a lyrics preview that
//! follows the song, and About the artist.
//!
//! Everything here comes from what the app already holds or reads through
//! the playback session; the artist card's picture, verified mark and
//! listeners come from the same overview the artist page reads, which uses
//! no Web API quota.

use egui::{Align2, Frame, Margin, Rect, Sense, pos2, vec2};

use crate::app::App;
use crate::i18n::gettext;
use crate::model::{Action, Loadable, Page};
use crate::theme::{self, Icon};

use super::widgets;

pub fn side_panel(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let locale = app.locale;
    let fit = super::yielding_panel(
        ui.ctx(),
        "now-playing-panel",
        theme::SIDE_PANEL_MIN_WIDTH..=560.0,
        app.settings.queue_width,
        ui.available_width() - super::topbar::least_width(ui.ctx()),
    );
    let panel = egui::Panel::right("now-playing-panel")
        .resizable(true)
        .default_size(app.settings.queue_width)
        .size_range(fit.range.clone())
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(palette.panel)
                .corner_radius(8.0)
                .outer_margin(Margin {
                    left: 0,
                    right: 8,
                    top: 0,
                    bottom: 0,
                })
                .inner_margin(Margin::symmetric(16, 16)),
        );
    let response = panel.show(ui, |ui| {
        let window_controls =
            super::window_controls_reservation(ui.ctx(), true, false, ui.available_width());
        ui.add_space(window_controls.queue_top);
        let mut close = false;
        let title = app
            .now_playing()
            .map(|now| now.album_name)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| gettext(locale, "Now playing").to_string());
        egui::Sides::new().shrink_left().show(
            ui,
            |ui| {
                ui.add_space(4.0);
                theme::text(ui, title, theme::bold(16.0), palette.text);
            },
            |ui| {
                close = theme::icon_button(
                    ui,
                    Icon::X,
                    18.0,
                    palette.secondary,
                    palette.text,
                    &gettext(locale, "Close"),
                )
                .clicked();
            },
        );
        if close {
            app.actions.push(Action::ToggleNowPlayingPanel);
        }
        ui.add_space(12.0);
        egui::ScrollArea::vertical()
            .id_salt("now-playing-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| contents(app, ui));
    });
    // Shares the Queue panel's width, so switching between them never moves
    // the page.
    let width = response.response.rect.width();
    if (width - app.settings.queue_width).abs() > 1.0
        && super::panel_width_chosen(ui.ctx(), "now-playing-panel", &fit)
    {
        app.settings.queue_width = width;
        app.actions.push(Action::SettingsChanged);
    }
}

fn contents(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let locale = app.locale;
    let Some(now) = app.now_playing() else {
        theme::subtle(ui, &palette, &gettext(locale, "Nothing is playing."));
        return;
    };
    let width = ui.available_width();

    // The cover, as wide as the panel.
    let (cover, cover_response) = ui.allocate_exact_size(vec2(width, width), Sense::click());
    widgets::paint_cover(
        ui,
        &palette,
        now.art_url.as_deref().or(now.art_small.as_deref()),
        cover,
        8.0,
        Icon::Music,
        None,
    );
    if cover_response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
        && let Some(album) = &now.album_id
    {
        app.actions.push(Action::Open(Page::Album(album.clone())));
    }
    ui.add_space(14.0);

    // The song and its save mark.
    // Fork: Spotify's 24px title with the mark at the panel's right edge,
    // the artists close under it.
    ui.spacing_mut().item_spacing.y = 0.0;
    let width = ui.available_width();
    let row = ui.allocate_ui_with_layout(
        vec2(width, 30.0),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
        if !now.is_episode {
            let saved = app.is_saved(&now.uri).unwrap_or(false);
            let (rect, response) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
            if saved {
                ui.painter().circle_filled(rect.center(), 10.0, palette.accent);
                theme::paint_icon(ui, Icon::Check, rect, 13.0, egui::Color32::from_gray(10));
            } else {
                let tint = if response.hovered() {
                    palette.text
                } else {
                    palette.secondary
                };
                theme::paint_icon(ui, Icon::CirclePlus, rect, 20.0, tint);
            }
            if response
                .on_hover_text(if saved {
                    gettext(locale, "Remove from Liked Songs")
                } else {
                    gettext(locale, "Save to Liked Songs")
                })
                .clicked()
            {
                app.actions.push(Action::ToggleSaved(now.uri.clone()));
            }
        }
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&now.title)
                        .font(theme::bold(24.0))
                        .color(palette.text),
                )
                .truncate(),
            );
        });
        },
    );
    let _ = row;
    ui.add_space(2.0);
    // The artists, each a link.
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (index, artist) in now.artists.iter().enumerate() {
            if index > 0 {
                theme::text(ui, ", ", theme::regular(14.0), palette.secondary);
            }
            if theme::link(ui, &artist.name, theme::regular(16.0), palette.secondary).clicked()
                && let Some(id) = &artist.id
            {
                app.actions.push(Action::Open(Page::Artist(id.clone())));
            }
        }
        if now.artists.is_empty() && !now.subtitle.is_empty() {
            theme::text(ui, &now.subtitle, theme::regular(16.0), palette.secondary);
        }
    });
    ui.spacing_mut().item_spacing.y = 6.0;
    ui.add_space(18.0);

    lyrics_card(app, ui, &now);
    if let Some(artist) = now.artists.first() {
        about_artist(app, ui, artist);
    }
}

/// The lines around the one being sung, in a coloured card; a click opens
/// the Lyrics panel.
fn lyrics_card(app: &mut App, ui: &mut egui::Ui, now: &crate::app::NowPlaying) {
    let palette = app.palette;
    let locale = app.locale;
    if now.is_episode {
        return;
    }
    // Asked once per song: a failed read is not asked again every frame.
    if app.lyrics_uri.as_deref() != Some(now.uri.as_str()) {
        app.request_lyrics();
    }
    let lines: Vec<(String, bool)> = match &app.lyrics {
        Loadable::Loaded(Some(lyrics)) if !lyrics.instrumental && !lyrics.lines.is_empty() => {
            let active = lyrics.active_line(now.position_ms);
            let start = active.unwrap_or(0);
            lyrics
                .lines
                .iter()
                .enumerate()
                .skip(start)
                .filter(|(_, line)| !line.text.trim().is_empty())
                .take(4)
                .map(|(index, line)| (line.text.clone(), Some(index) == active))
                .collect()
        }
        _ => return,
    };
    if lines.is_empty() {
        return;
    }
    let width = ui.available_width();
    let fill = palette.tint.gamma_multiply(0.9);
    let response = Frame::new()
        .fill(if fill.a() == 0 { palette.surface } else { fill })
        .corner_radius(8.0)
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(width - 32.0);
            theme::text(
                ui,
                gettext(locale, "Lyrics preview"),
                theme::bold(15.0),
                palette.text,
            );
            ui.add_space(10.0);
            for (text, active) in lines {
                let color = if active {
                    palette.text
                } else {
                    palette.text.gamma_multiply(0.55)
                };
                ui.add(
                    egui::Label::new(egui::RichText::new(text).font(theme::bold(20.0)).color(color))
                        .wrap(),
                );
            }
        })
        .response
        .interact(Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        app.actions.push(Action::ToggleLyricsPanel);
    }
    ui.add_space(16.0);
}

/// The first artist's picture, name, monthly listeners and Follow, as
/// Spotify's About the artist card.
fn about_artist(app: &mut App, ui: &mut egui::Ui, artist: &crate::api::models::ArtistRef) {
    let palette = app.palette;
    let locale = app.locale;
    let Some(id) = artist.id.clone() else {
        return;
    };
    let uri = artist
        .uri
        .clone()
        .unwrap_or_else(|| format!("spotify:artist:{id}"));
    // The overview is read once and kept for the account.
    app.actions.push(Action::RequestPlayCounts(uri.clone()));
    let visuals = app.artist_visuals.get(&uri).cloned().unwrap_or_default();
    let picture = visuals.header.clone().or_else(|| {
        app.known_artist(&id)
            .and_then(|known| crate::api::models::pick_image(&known.images, 640).map(str::to_owned))
    });
    let listeners = app.play_counts.get(&uri).copied();
    let width = ui.available_width();
    let frame = Frame::new()
        .fill(palette.surface)
        .corner_radius(8.0)
        .show(ui, |ui| {
            ui.set_width(width);
            // The picture across the top, with the heading over it.
            let height = (width * 0.62).round();
            let (art, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
            widgets::paint_cover(
                ui,
                &palette,
                picture.as_deref(),
                art,
                8.0,
                Icon::Mic,
                None,
            );
            let shade = Rect::from_min_max(art.min, pos2(art.right(), art.top() + 44.0));
            let mut mesh = egui::Mesh::default();
            let dark = egui::Color32::from_black_alpha(150);
            mesh.colored_vertex(shade.left_top(), dark);
            mesh.colored_vertex(shade.right_top(), dark);
            mesh.colored_vertex(shade.right_bottom(), egui::Color32::TRANSPARENT);
            mesh.colored_vertex(shade.left_bottom(), egui::Color32::TRANSPARENT);
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            ui.painter().add(egui::Shape::mesh(mesh));
            ui.painter().text(
                art.left_top() + vec2(16.0, 16.0),
                Align2::LEFT_TOP,
                gettext(locale, "About the artist"),
                theme::bold(15.0),
                egui::Color32::WHITE,
            );
            egui::Frame::new()
                .inner_margin(Margin::same(16))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if theme::link(ui, &artist.name, theme::bold(17.0), palette.text).clicked() {
                            app.actions.push(Action::Open(Page::Artist(id.clone())));
                        }
                        if visuals.verified {
                            let (badge, _) =
                                ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                            ui.painter().circle_filled(
                                badge.center(),
                                8.0,
                                egui::Color32::from_rgb(77, 145, 255),
                            );
                            theme::paint_icon(ui, Icon::Check, badge, 10.0, egui::Color32::WHITE);
                        }
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if let Some(listeners) = listeners {
                            theme::text(
                                ui,
                                format!(
                                    "{} {}",
                                    crate::util::format_count(listeners),
                                    gettext(locale, "monthly listeners")
                                ),
                                theme::regular(14.0),
                                palette.secondary,
                            );
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let following = app.is_saved(&uri).unwrap_or(false);
                            let label = if following {
                                crate::i18n::pgettext(locale, "artist", "Following")
                            } else {
                                crate::i18n::pgettext(locale, "artist", "Follow")
                            };
                            if theme::pill_button(ui, &palette, &label, false).clicked() {
                                app.actions.push(Action::ToggleSaved(uri.clone()));
                            }
                        });
                    });
                });
        });
    let _ = frame;
    ui.add_space(16.0);
}

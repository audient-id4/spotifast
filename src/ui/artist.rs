//! The artist page.

use std::sync::Arc;

use crate::api::models::{Artist, PlayableItem, pick_image};
use crate::app::App;
use crate::i18n::{Locale, gettext, ngettext, pgettext};
use crate::model::{Action, DiscographyFilter, Loadable, Page, RowContext};
use crate::theme::{self, Icon};
use crate::util;

use super::collection::{Hero, HeroArt, hero, hero_images};
use super::widgets::{self, TrackRow};

/// Draws the artist page and keeps discography loading placeholders in place.
pub fn show(app: &mut App, ui: &mut egui::Ui, id: &str) {
    if !app.artist_pages.contains_key(id) {
        app.ensure_loaded(Page::Artist(id.to_string()));
    }
    let Some(page) = app.artist_pages.remove(id) else {
        return;
    };
    let preview =
        super::loading_preview(ui.ctx(), id, &page.artist, || app.known_artist(id).cloned());
    let palette = app.palette;
    let locale = app.locale;
    match &page.artist {
        Loadable::Loaded(artist) => {
            artist_hero(app, ui, artist, preview.as_deref());
            artist_actions(app, ui, artist);
            // The same overview supplies both play counts and the upcoming release.
            app.actions
                .push(Action::RequestPlayCounts(artist.uri.clone()));
            // Popular.
            theme::section_title(ui, &palette, &gettext(locale, "Popular"));
            ui.add_space(4.0);
            match &page.top_tracks {
                Loadable::Loaded(tracks) if !tracks.is_empty() => {
                    let uris: Arc<[String]> = tracks
                        .iter()
                        .map(|track| track.uri.clone())
                        .collect::<Vec<_>>()
                        .into();
                    let context = RowContext::Uris(Arc::clone(&uris));
                    let items: Vec<PlayableItem> =
                        tracks.iter().cloned().map(PlayableItem::Track).collect();
                    let limit = if page.show_all_top { items.len() } else { 5 };
                    let show_plays = app.has_play_counts(&artist.uri);
                    let render_top = |ui: &mut egui::Ui, app: &mut App| {
                        for (index, item) in items.iter().take(limit).enumerate() {
                            widgets::track_row(
                                ui,
                                app,
                                TrackRow {
                                    index,
                                    number: Some(index + 1),
                                    item,
                                    context: &context,
                                    show_cover: !app.settings.tracklist_compact,
                                    show_album: false,
                                    added_at: None,
                                    added_by: None,
                                    show_added_by: false,
                                    show_plays,
                                    compact: false,
                                    thin: app.settings.tracklist_compact,
                                    shift: 0.0,
                                    picked: false,
                                    picked_songs: &[],
                                    picked_positions: &[],
                                    position: None,
                                },
                            );
                        }
                    };
                    if !app.network_reachable {
                        ui.add_enabled_ui(false, |ui| {
                            ui.multiply_opacity(0.55);
                            render_top(ui, app);
                        });
                    } else {
                        render_top(ui, app);
                    }
                    if items.len() > 5 {
                        ui.add_space(6.0);
                        if theme::soft_button(
                            ui,
                            &palette,
                            None,
                            &if page.show_all_top {
                                gettext(locale, "Show less")
                            } else {
                                gettext(locale, "See more")
                            },
                            false,
                        )
                        .clicked()
                        {
                            app.actions.push(Action::ToggleShowAllTop(id.to_string()));
                        }
                    }
                }
                Loadable::Loaded(_) => {
                    theme::subtle(ui, &palette, &gettext(locale, "No popular songs to show."));
                }
                Loadable::Loading | Loadable::NotLoaded => {
                    widgets::skeleton_rows(ui, &palette, app.locale, 5)
                }
                Loadable::Failed(error) => {
                    let error = error.clone();
                    widgets::error_row(ui, app, &error, None);
                }
            }
            ui.add_space(20.0);
            you_liked(app, ui, artist);

            if let Some(upcoming) = app.artist_prereleases.get(&artist.uri).cloned()
                && upcoming.is_upcoming(jiff::Timestamp::now().as_second())
            {
                upcoming_release(app, ui, &upcoming);
                ui.add_space(32.0);
            }

            // Discography.
            theme::section_title(ui, &palette, &gettext(locale, "Discography"));
            ui.add_space(6.0);
            let labels: Vec<_> = DiscographyFilter::ALL
                .iter()
                .map(|f| (*f, f.label(locale)))
                .collect();
            let options: Vec<(DiscographyFilter, &str)> = labels
                .iter()
                .map(|(filter, label)| (*filter, label.as_ref()))
                .collect();
            if let Some(filter) = widgets::chips(ui, &palette, &options, page.filter) {
                app.actions.push(Action::SetDiscographyFilter {
                    artist_id: id.to_string(),
                    filter,
                });
            }
            ui.add_space(10.0);
            // Reserve the previous list's height while a new category loads.
            // Its cards disappear immediately so they always match the chip.
            let height_id = ui.make_persistent_id(("discography-height", id));
            let list = page.albums.get(page.filter.groups());
            let loading = list.is_none_or(|list| !list.loaded_once && list.error.is_none());
            let previous = ui
                .ctx()
                .data(|data| data.get_temp::<(f32, usize)>(height_id));
            let discography = ui.scope(|ui| {
                if loading {
                    ui.set_min_height(previous.map_or(0.0, |(height, _)| height));
                }
                match list {
                    Some(list) if !loading => {
                        let mut seen = std::collections::HashSet::new();
                        let albums: Vec<_> = list
                            .items
                            .iter()
                            .filter(|album| seen.insert(album.name.to_lowercase()))
                            .collect();
                        widgets::grid(ui, |ui| {
                            for album in &albums {
                                let subtitle = format!(
                                    "{} • {}",
                                    album.year().unwrap_or(""),
                                    app.album_kind_label(album)
                                );
                                let context_here = app.playing_context_uri().as_deref()
                                    == Some(album.uri.as_str());
                                let playing_here = context_here && app.believed_playing();
                                let card_art = app.art_px(widgets::CARD_ART);
                                let card = widgets::card(
                                    ui,
                                    app,
                                    pick_image(&album.images, card_art),
                                    &album.name,
                                    subtitle.trim_start_matches(" • "),
                                    widgets::CardCover::square(playing_here),
                                );
                                if card.play {
                                    if context_here {
                                        app.actions.push(Action::TogglePlay);
                                    } else {
                                        app.actions.push(Action::PlayContext {
                                            uri: album.uri.clone(),
                                            offset_uri: None,
                                            offset_index: None,
                                        });
                                    }
                                }
                                if card.clicked {
                                    app.actions
                                        .push(Action::Open(Page::Album(album.id.clone())));
                                }
                                egui::Popup::context_menu(&card.response)
                                    .id(ui.make_persistent_id(("discography-menu", &album.uri)))
                                    .frame(widgets::menu_frame(&palette))
                                    .show(|ui| {
                                        widgets::context_menu_items(
                                            ui,
                                            app,
                                            &album.uri,
                                            &album.name,
                                            None,
                                        )
                                    });
                            }
                        });
                        if list.loading {
                            widgets::loading_row(ui, &palette, app.locale);
                        } else if let Some(error) = &list.error {
                            let error = error.clone();
                            widgets::error_row(ui, app, &error, None);
                        } else if list.items.is_empty() {
                            theme::subtle(
                                ui,
                                &palette,
                                &gettext(locale, "Nothing in this category."),
                            );
                        } else if list.can_load_more() {
                            ui.add_space(8.0);
                            if theme::soft_button(
                                ui,
                                &palette,
                                None,
                                &gettext(locale, "Load more"),
                                false,
                            )
                            .clicked()
                            {
                                app.actions
                                    .push(Action::LoadMoreArtistAlbums(id.to_string()));
                            }
                        }
                        albums.len()
                    }
                    _ => {
                        let count = previous.map_or(6, |(_, count)| count.max(1));
                        widgets::grid(ui, |ui| {
                            widgets::skeleton_cards_filling(
                                ui,
                                &palette,
                                app.locale,
                                count,
                                false,
                                previous.map_or(0.0, |(height, _)| height),
                            )
                        });
                        count
                    }
                }
            });
            if !loading {
                ui.ctx().data_mut(|data| {
                    data.insert_temp(
                        height_id,
                        (discography.response.rect.height(), discography.inner),
                    );
                });
            }
            ui.add_space(20.0);

            // Related.
            if let Loadable::Loaded(related) = &page.related
                && !related.is_empty()
            {
                let artist_label = gettext(locale, "Artist");
                let title = gettext(locale, "Fans also like");
                widgets::shelf(ui, &palette, "related", &title, |ui| {
                    for artist in related {
                        let context_here =
                            app.playing_context_uri().as_deref() == Some(artist.uri.as_str());
                        let playing_here = context_here && app.believed_playing();
                        let card_art = app.art_px(widgets::CARD_ART);
                        let card = widgets::card(
                            ui,
                            app,
                            pick_image(&artist.images, card_art),
                            &artist.name,
                            &artist_label,
                            widgets::CardCover::portrait(playing_here),
                        );
                        if card.play {
                            if context_here {
                                app.actions.push(Action::TogglePlay);
                            } else {
                                app.actions.push(Action::PlayContext {
                                    uri: artist.uri.clone(),
                                    offset_uri: None,
                                    offset_index: None,
                                });
                            }
                        }
                        if card.clicked {
                            app.actions
                                .push(Action::Open(Page::Artist(artist.id.clone())));
                        }
                        egui::Popup::context_menu(&card.response)
                            .id(ui.make_persistent_id(("related-artist-menu", &artist.uri)))
                            .frame(widgets::menu_frame(&palette))
                            .show(|ui| {
                                widgets::context_menu_items(
                                    ui,
                                    app,
                                    &artist.uri,
                                    &artist.name,
                                    None,
                                )
                            });
                    }
                });
            }
        }
        Loadable::Loading | Loadable::NotLoaded => {
            if let Some(artist) = &preview {
                artist_hero(app, ui, artist, None);
            } else {
                ui.add_space(40.0);
            }
            if !app.network_reachable {
                widgets::empty_state(
                    ui,
                    &palette,
                    Icon::WifiOff,
                    &gettext(app.locale, "You're offline"),
                    &gettext(
                        app.locale,
                        "Spotifast will reconnect automatically when your connection returns.",
                    ),
                );
            } else {
                if let Some(artist) = &preview {
                    ui.add_enabled_ui(false, |ui| artist_actions(app, ui, artist));
                }
                widgets::skeleton_rows(ui, &palette, app.locale, 5);
            }
        }
        Loadable::Failed(error) => {
            let error = error.clone();
            if let Some(artist) = &preview {
                artist_hero(app, ui, artist, None);
            } else {
                ui.add_space(40.0);
            }
            if !app.network_reachable
                || error.contains("network error")
                || error.contains("Network connection error")
            {
                widgets::empty_state(
                    ui,
                    &palette,
                    Icon::WifiOff,
                    &gettext(app.locale, "You're offline"),
                    &gettext(
                        app.locale,
                        "Spotifast will reconnect automatically when your connection returns.",
                    ),
                );
            } else {
                if let Some(artist) = &preview {
                    ui.add_enabled_ui(false, |ui| artist_actions(app, ui, artist));
                }
                widgets::error_row(ui, app, &error, Some(Page::Artist(id.to_string())));
            }
        }
    }
    app.artist_pages.insert(id.to_string(), page);
}

fn upcoming_release(
    app: &mut App,
    ui: &mut egui::Ui,
    release: &crate::prerelease::UpcomingRelease,
) {
    let palette = app.palette;
    let heading = gettext(app.locale, "Release countdown");
    theme::section_title(ui, &palette, &heading);
    ui.add_space(12.0);
    let album_label = gettext(app.locale, "Album");
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().min(440.0), 96.0),
        egui::Sense::click(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            format!("{heading}: {}", release.name),
        )
    });
    if response.gained_focus() {
        response.scroll_to_me(None);
    }
    ui.painter().rect_filled(
        rect,
        theme::RADIUS as f32,
        if response.hovered() || response.has_focus() {
            palette.surface_hover
        } else {
            egui::Color32::TRANSPARENT
        },
    );
    let cover = egui::Rect::from_min_size(rect.min + egui::vec2(0.0, 4.0), egui::Vec2::splat(88.0));
    widgets::paint_cover(
        ui,
        &palette,
        pick_image(&release.images, app.art_px(88.0)),
        cover,
        4.0,
        Icon::Music,
        Some(app.backend.art()),
    );
    let left = cover.right() + 12.0;
    let right = rect.right() - 12.0;
    for (text, y, font, color) in [
        (
            release.name.as_str(),
            28.0,
            theme::semibold(16.0),
            palette.text,
        ),
        (
            album_label.as_ref(),
            52.0,
            theme::regular(13.0),
            palette.secondary,
        ),
    ] {
        let galley = widgets::ellipsized(ui, text, font, color, (right - left).max(1.0), 1);
        let x = if galley.job.halign == egui::Align::RIGHT {
            right
        } else {
            left
        };
        ui.painter()
            .galley(egui::pos2(x, rect.top() + y), galley, color);
    }
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(&release.name)
        .clicked()
    {
        app.actions.push(Action::OpenLink(release.uri.clone()));
    }
}

fn artist_hero(app: &mut App, ui: &mut egui::Ui, artist: &Artist, preview: Option<&Artist>) {
    let locale = app.locale;
    let byline = artist_byline(locale, artist, app.play_counts.get(&artist.uri).copied());
    // Fork: Spotify's banner when the artist has a header picture.
    if let Some(visuals) = app.artist_visuals.get(&artist.uri).cloned()
        && let Some(header) = visuals.header.as_deref()
    {
        let listeners = byline.first().map(|(text, _)| text.clone());
        banner(
            app,
            ui,
            artist,
            header,
            visuals.verified,
            listeners.as_deref(),
        );
        return;
    }
    let images = hero_images(
        &artist.images,
        preview.map(|artist| artist.images.as_slice()),
        false,
        HeroArt::of(app),
    );
    hero(
        app,
        ui,
        Hero {
            images,
            liked: false,
            kind: gettext(locale, "Artist"),
            title: &artist.name,
            description: None,
            byline,
            byline_tooltip: None,
            avatar: None,
            round: true,
        },
    );
}

/// Monthly listeners, which Spotify's playback session reads with the
/// popular songs' play counts, then followers and genres.
fn artist_byline(
    locale: Locale,
    artist: &Artist,
    monthly_listeners: Option<u64>,
) -> Vec<(String, Option<Page>)> {
    let mut byline = Vec::new();
    if let Some(listeners) = monthly_listeners {
        byline.push((
            ngettext(
                locale,
                // Translators: {count} is the number of people who listened
                // to an artist in the last 28 days, as Spotify counts them.
                "{count} monthly listener",
                "{count} monthly listeners",
                u32::try_from(listeners).unwrap_or(u32::MAX),
            )
            .replace("{count}", &util::format_count(listeners)),
            None,
        ));
    }
    if let Some(followers) = &artist.followers {
        byline.push((
            ngettext(
                locale,
                // Translators: {count} is the number of people who follow an artist.
                "{count} follower",
                "{count} followers",
                u32::try_from(followers.total).unwrap_or(u32::MAX),
            )
            .replace("{count}", &util::format_count(followers.total)),
            None,
        ));
    }
    if !artist.genres.is_empty() {
        byline.push((
            artist
                .genres
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
            None,
        ));
    }
    byline
}

/// Fork: the wide header picture with the name over it, as Spotify's artist
/// page has: a dark fade at the foot for the words, the verified mark, the
/// name as large as the width allows and the monthly listeners.
fn banner(
    app: &mut App,
    ui: &mut egui::Ui,
    artist: &Artist,
    header: &str,
    verified: bool,
    listeners: Option<&str>,
) {
    let palette = app.palette;
    let width = ui.available_width();
    // Spotify's banner is a third of the panel's width tall.
    let height = (width * 0.34).clamp(240.0, 500.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    widgets::paint_cover(ui, &palette, Some(header), rect, 8.0, Icon::Music, None);
    // The fade: clear at the top, dark under the words.
    let fade_top = rect.top() + height * 0.35;
    let mut mesh = egui::Mesh::default();
    let dark = egui::Color32::from_black_alpha(190);
    let clear = egui::Color32::TRANSPARENT;
    mesh.colored_vertex(egui::pos2(rect.left(), fade_top), clear);
    mesh.colored_vertex(egui::pos2(rect.right(), fade_top), clear);
    mesh.colored_vertex(rect.right_bottom(), dark);
    mesh.colored_vertex(rect.left_bottom(), dark);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(egui::Shape::mesh(mesh));

    let left = rect.left() + 32.0;
    let mut y = rect.bottom() - 24.0;
    if let Some(listeners) = listeners {
        ui.painter().text(
            egui::pos2(left, y),
            egui::Align2::LEFT_BOTTOM,
            listeners,
            theme::regular(15.0),
            egui::Color32::WHITE,
        );
        y -= 30.0;
    }
    // The name shrinks until it fits the banner, as Spotify's does.
    let room = width - 64.0;
    let mut size = 96.0;
    let galley = loop {
        let galley = ui.painter().layout_no_wrap(
            artist.name.clone(),
            theme::bold(size),
            egui::Color32::WHITE,
        );
        if galley.size().x <= room || size <= 36.0 {
            break galley;
        }
        size -= 4.0;
    };
    let name_top = y - galley.size().y;
    ui.painter()
        .galley(egui::pos2(left, name_top), galley, egui::Color32::WHITE);
    if verified {
        let badge_y = name_top - 4.0;
        let badge =
            egui::Rect::from_min_size(egui::pos2(left, badge_y - 22.0), egui::vec2(22.0, 22.0));
        ui.painter()
            .circle_filled(badge.center(), 11.0, egui::Color32::from_rgb(61, 145, 244));
        theme::paint_icon(ui, Icon::Check, badge, 13.0, egui::Color32::WHITE);
        ui.painter().text(
            egui::pos2(badge.right() + 8.0, badge.center().y),
            egui::Align2::LEFT_CENTER,
            gettext(app.locale, "Verified by Spotify"),
            theme::medium(14.0),
            egui::Color32::WHITE,
        );
    }
    ui.add_space(20.0);
    // Past the banner, the page keeps a bar with the name and Play.
    let gone = rect.bottom() < ui.clip_rect().top() + 72.0;
    if gone {
        sticky_bar(app, ui, artist);
    }
}

/// Fork: the bar Spotify keeps at the top of an artist page scrolled past its
/// banner: the Play button and the name.
fn sticky_bar(app: &mut App, ui: &mut egui::Ui, artist: &Artist) {
    let palette = app.palette;
    let clip = ui.clip_rect();
    let bar = egui::Rect::from_min_size(clip.min, egui::vec2(clip.width(), 64.0));
    egui::Area::new(egui::Id::new(("artist-sticky", &artist.uri)))
        .order(egui::Order::Foreground)
        .fixed_pos(bar.min)
        .show(ui.ctx(), |ui| {
            let (rect, _) = ui.allocate_exact_size(bar.size(), egui::Sense::hover());
            ui.painter().rect_filled(rect, 0.0, palette.panel);
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect.shrink2(egui::vec2(24.0, 8.0)))
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            child.spacing_mut().item_spacing.x = 16.0;
            if theme::circle_button(
                &mut child,
                Icon::PlayFilled,
                44.0,
                palette.accent,
                palette.accent_hover,
                palette.on_accent,
                &gettext(app.locale, "Play"),
            )
            .clicked()
            {
                app.actions.push(Action::PlayContext {
                    uri: artist.uri.clone(),
                    offset_uri: None,
                    offset_index: None,
                });
            }
            child.label(
                egui::RichText::new(&artist.name)
                    .font(theme::bold(24.0))
                    .color(palette.text),
            );
        });
}

fn artist_actions(app: &mut App, ui: &mut egui::Ui, artist: &Artist) {
    let palette = app.palette;
    let locale = app.locale;
    let following = app.is_saved(&artist.uri).unwrap_or(false);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 18.0;
        if app.play_pending(&artist.uri) {
            theme::circle_spinner(
                ui,
                56.0,
                palette.accent,
                palette.on_accent,
                &gettext(locale, "Starting…"),
            );
        } else if theme::circle_button(
            ui,
            Icon::PlayFilled,
            56.0,
            palette.accent,
            palette.accent_hover,
            palette.on_accent,
            &gettext(locale, "Play"),
        )
        .clicked()
        {
            app.actions.push(Action::PlayContext {
                uri: artist.uri.clone(),
                offset_uri: None,
                offset_index: None,
            });
        }
        // Fork: shuffle beside Play, as Spotify's artist page has it.
        let shuffle = app.playing_context_shuffle();
        if theme::icon_button(
            ui,
            Icon::Shuffle,
            28.0,
            if shuffle {
                palette.accent
            } else {
                palette.secondary
            },
            if shuffle {
                palette.accent_hover
            } else {
                palette.text
            },
            &gettext(locale, "Shuffle"),
        )
        .clicked()
        {
            app.actions.push(Action::ToggleShuffle);
        }
        if theme::pill_button(
            ui,
            &palette,
            &if following {
                pgettext(locale, "artist", "Following")
            } else {
                pgettext(locale, "artist", "Follow")
            },
            false,
        )
        .clicked()
        {
            app.actions.push(Action::ToggleSaved(artist.uri.clone()));
        }
        let more = theme::icon_button(
            ui,
            Icon::Ellipsis,
            26.0,
            palette.secondary,
            palette.text,
            &gettext(locale, "More"),
        );
        egui::Popup::menu(&more)
            .frame(widgets::menu_frame(&palette))
            .show(|ui| widgets::context_menu_items(ui, app, &artist.uri, &artist.name, None));
    });
    ui.add_space(20.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::Followers;

    #[test]
    fn the_byline_leads_with_monthly_listeners_once_spotify_gives_them() {
        let artist = Artist {
            followers: Some(Followers { total: 1_200 }),
            genres: vec!["indie".into()],
            ..Artist::default()
        };
        let texts = |listeners| {
            artist_byline(Locale::English, &artist, listeners)
                .into_iter()
                .map(|(text, _)| text)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            texts(Some(8_123_456)),
            ["8,123,456 monthly listeners", "1,200 followers", "indie"]
        );
        assert_eq!(texts(None), ["1,200 followers", "indie"]);
    }
}

/// Fork: the liked songs with this artist on them, as Spotify's artist page
/// shows under You Liked. A click opens Liked Songs with only those songs.
fn you_liked(app: &mut App, ui: &mut egui::Ui, artist: &crate::api::models::Artist) {
    let palette = app.palette;
    let locale = app.locale;
    let count = app
        .library
        .liked
        .items
        .iter()
        .filter(|saved| {
            saved
                .track
                .artists
                .iter()
                .any(|a| a.id.as_deref() == Some(artist.id.as_str()))
        })
        .count();
    if count == 0 {
        return;
    }
    theme::section_title(ui, &palette, &gettext(locale, "You Liked"));
    ui.add_space(8.0);
    let size = 96.0;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().min(420.0), size + 8.0),
        egui::Sense::click(),
    );
    if response.hovered() {
        ui.painter().rect_filled(rect, 8.0, palette.surface_hover);
    }
    let art = egui::Rect::from_min_size(rect.min + egui::vec2(4.0, 4.0), egui::Vec2::splat(size));
    let url = crate::api::models::pick_image(&artist.images, 160);
    widgets::paint_cover(ui, &palette, url, art, size / 2.0, Icon::Music, None);
    // The green heart on the picture's lower right, as Spotify draws it.
    let badge = art.right_bottom() - egui::vec2(14.0, 14.0);
    ui.painter().circle_filled(badge, 12.0, palette.window);
    let heart = egui::Rect::from_center_size(badge, egui::Vec2::splat(18.0));
    theme::paint_icon(ui, Icon::HeartFilled, heart, 14.0, palette.accent);
    let text_left = art.right() + 16.0;
    let songs = crate::i18n::ngettext(locale, "{count} song", "{count} songs", count as u32)
        .replace("{count}", &crate::util::format_count(count as u64));
    ui.painter().text(
        egui::pos2(text_left, rect.center().y - 10.0),
        egui::Align2::LEFT_CENTER,
        songs,
        theme::bold(15.0),
        palette.text,
    );
    ui.painter().text(
        egui::pos2(text_left, rect.center().y + 12.0),
        egui::Align2::LEFT_CENTER,
        format!("By {}", artist.name),
        theme::regular(13.0),
        palette.secondary,
    );
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        app.liked_tags.artist = Some((artist.id.clone(), artist.name.clone()));
        app.liked_tags.duplicates = false;
        app.actions.push(Action::Open(Page::LikedSongs));
    }
    ui.add_space(28.0);
}

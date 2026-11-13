//! Fork: listening statistics from Spotify's streaming history export.
//!
//! Spotify's API gives no play counts of one's own. Its privacy export,
//! "Extended streaming history", does: JSON files of every play, each with
//! the time it ended (`ts`, UTC), how long it played (`ms_played`), the song,
//! its album artist and URI. Put the files in `streaming-history` in the
//! settings folder and this reads them all.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// A play counts after 30 seconds, as Spotify counts streams.
const COUNTS_AFTER_MS: u64 = 30_000;
const TOP_ARTISTS: usize = 25;
const TOP_SONGS: usize = 50;

pub fn folder(dirs: &crate::paths::AppDirs) -> PathBuf {
    dirs.config.join("streaming-history")
}

/// One row of the export. Podcast rows carry no song and are skipped.
#[derive(Deserialize)]
struct Row {
    ts: String,
    #[serde(default)]
    ms_played: u64,
    #[serde(default)]
    master_metadata_track_name: Option<String>,
    #[serde(default)]
    master_metadata_album_artist_name: Option<String>,
    #[serde(default)]
    spotify_track_uri: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Song {
    pub uri: String,
    pub name: String,
    pub artist: String,
    pub plays: u32,
    pub ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Artist {
    pub name: String,
    pub plays: u32,
    pub ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    pub files: usize,
    /// Plays of 30 seconds or more.
    pub plays: u64,
    pub total_ms: u64,
    pub songs: usize,
    pub artists: usize,
    /// First and last day in the export, `YYYY-MM-DD`, local time.
    pub first: String,
    pub last: String,
    /// Listening time by month, `YYYY-MM`, oldest first.
    pub months: Vec<(String, u64)>,
    /// Listening time by local hour of day.
    pub hours: [u64; 24],
    /// Listening time by weekday, Monday first.
    pub weekdays: [u64; 7],
    pub top_artists: Vec<Artist>,
    pub top_songs: Vec<Song>,
    /// Every song's play count, by URI.
    pub counts: HashMap<String, u32>,
}

/// Every export file in the folder: the `.json` files whose name says audio,
/// or any `.json` when none does.
fn files(folder: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut all: Vec<PathBuf> = std::fs::read_dir(folder)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        })
        .collect();
    all.sort();
    let audio: Vec<PathBuf> = all
        .iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains("Audio") || name.contains("StreamingHistory"))
        })
        .cloned()
        .collect();
    Ok(if audio.is_empty() { all } else { audio })
}

/// Reads the whole folder. Slow for big exports: run it off the UI thread.
pub fn read(folder: &Path) -> Result<Stats, String> {
    let paths = files(folder).map_err(|_| "no_folder".to_string())?;
    if paths.is_empty() {
        return Err("empty".into());
    }
    let mut rows = Vec::new();
    for path in &paths {
        let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
        // A file that is no play list (the export's other JSON) is skipped.
        if let Ok(mut file) = serde_json::from_slice::<Vec<Row>>(&bytes) {
            rows.append(&mut file);
        }
    }
    let mut stats = summarize(rows, jiff::tz::TimeZone::system());
    stats.files = paths.len();
    if stats.total_ms == 0 {
        return Err("empty".into());
    }
    Ok(stats)
}

fn summarize(rows: Vec<Row>, zone: jiff::tz::TimeZone) -> Stats {
    let mut stats = Stats::default();
    let mut months: HashMap<String, u64> = HashMap::new();
    let mut artists: HashMap<String, Artist> = HashMap::new();
    let mut songs: HashMap<String, Song> = HashMap::new();
    let (mut first, mut last): (Option<jiff::civil::Date>, Option<jiff::civil::Date>) = (None, None);
    for row in rows {
        let (Some(name), Some(artist)) = (
            row.master_metadata_track_name,
            row.master_metadata_album_artist_name,
        ) else {
            continue;
        };
        let Ok(at) = row.ts.parse::<jiff::Timestamp>() else {
            continue;
        };
        let local = at.to_zoned(zone.clone());
        let date = local.date();
        first = Some(first.map_or(date, |d| d.min(date)));
        last = Some(last.map_or(date, |d| d.max(date)));
        let ms = row.ms_played;
        let counted = ms >= COUNTS_AFTER_MS;
        stats.total_ms += ms;
        if counted {
            stats.plays += 1;
        }
        *months
            .entry(format!("{:04}-{:02}", date.year(), date.month()))
            .or_default() += ms;
        stats.hours[local.hour() as usize] += ms;
        stats.weekdays[local.weekday().to_monday_zero_offset() as usize] += ms;

        let entry = artists.entry(artist.clone()).or_insert_with(|| Artist {
            name: artist.clone(),
            ..Default::default()
        });
        entry.ms += ms;
        entry.plays += u32::from(counted);

        let uri = row.spotify_track_uri.unwrap_or_default();
        let key = if uri.is_empty() {
            format!("{artist}\u{0}{name}")
        } else {
            uri.clone()
        };
        let song = songs.entry(key).or_insert_with(|| Song {
            uri,
            name,
            artist,
            ..Default::default()
        });
        song.ms += ms;
        song.plays += u32::from(counted);
    }
    stats.first = first.map(|d| d.to_string()).unwrap_or_default();
    stats.last = last.map(|d| d.to_string()).unwrap_or_default();
    let mut months: Vec<(String, u64)> = months.into_iter().collect();
    months.sort();
    stats.months = months;
    stats.artists = artists.len();
    stats.songs = songs.len();
    stats.counts = songs
        .values()
        .filter(|song| !song.uri.is_empty() && song.plays > 0)
        .map(|song| (song.uri.clone(), song.plays))
        .collect();
    let mut artists: Vec<Artist> = artists.into_values().collect();
    artists.sort_by(|a, b| b.ms.cmp(&a.ms).then_with(|| a.name.cmp(&b.name)));
    artists.truncate(TOP_ARTISTS);
    stats.top_artists = artists;
    let mut songs: Vec<Song> = songs.into_values().collect();
    songs.sort_by(|a, b| {
        b.plays
            .cmp(&a.plays)
            .then(b.ms.cmp(&a.ms))
            .then_with(|| a.name.cmp(&b.name))
    });
    songs.truncate(TOP_SONGS);
    stats.top_songs = songs;
    stats
}

/// `123 h` or `45 min` for a listening time.
pub fn hours_text(ms: u64) -> String {
    let minutes = ms / 60_000;
    if minutes >= 120 {
        format!("{} h", crate::util::format_count(minutes / 60))
    } else {
        format!("{minutes} min")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(ts: &str, ms: u64, song: &str, artist: &str, uri: &str) -> Row {
        Row {
            ts: ts.into(),
            ms_played: ms,
            master_metadata_track_name: Some(song.into()),
            master_metadata_album_artist_name: Some(artist.into()),
            spotify_track_uri: Some(uri.into()),
        }
    }

    #[test]
    fn counts_plays_over_thirty_seconds() {
        let rows = vec![
            row("2025-03-01T22:10:00Z", 180_000, "Эльфа", "Маленький ярче", "spotify:track:a"),
            row("2025-03-02T22:10:00Z", 200_000, "Эльфа", "Маленький ярче", "spotify:track:a"),
            row("2025-04-01T10:00:00Z", 10_000, "Эльфа", "Маленький ярче", "spotify:track:a"),
            row("2025-04-02T10:00:00Z", 150_000, "TORE UP", "Don Toliver", "spotify:track:b"),
            Row {
                ts: "2025-04-03T10:00:00Z".into(),
                ms_played: 999_000,
                master_metadata_track_name: None,
                master_metadata_album_artist_name: None,
                spotify_track_uri: None,
            },
        ];
        let stats = summarize(rows, jiff::tz::TimeZone::UTC);
        assert_eq!(stats.plays, 3);
        assert_eq!(stats.total_ms, 540_000);
        assert_eq!(stats.counts.get("spotify:track:a"), Some(&2));
        assert_eq!(stats.top_songs[0].name, "Эльфа");
        assert_eq!(stats.top_artists[0].name, "Маленький ярче");
        assert_eq!(
            stats.months,
            [("2025-03".to_string(), 380_000), ("2025-04".to_string(), 160_000)]
        );
        assert_eq!(stats.hours[22], 380_000);
        assert_eq!(stats.first, "2025-03-01");
        assert_eq!(stats.last, "2025-04-02");
    }
}

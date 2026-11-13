//! Genre and mood chips for Liked Songs, as Spotify's desktop client shows.
//!
//! Spotify's own client builds those chips on the device: every song carries
//! descriptors in extended metadata (`TRACK_DESCRIPTOR`), each a concept URI
//! with a localized term, a weight and a type such as genre or mood. The client
//! keeps the concepts that cover the most liked songs. This module reads those
//! descriptors, keeps them on disk by song URI so a song is asked about once,
//! and picks the chips.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use librespot_core::Session;
use librespot_protocol::extended_metadata::{BatchedEntityRequest, EntityRequest, ExtensionQuery};
use librespot_protocol::extension_kind::ExtensionKind;
use protobuf::EnumOrUnknown;
use serde::{Deserialize, Serialize};

/// Songs per extended metadata request.
const BATCH: usize = 200;
/// A descriptor weaker than this does not tag its song.
pub const MIN_WEIGHT: f32 = 0.5;
/// Spotify shows at most fifteen chips.
const MAX_CHIPS: usize = 15;
/// Spotify shows no chips for fewer liked songs than this.
const MIN_SONGS: usize = 30;
/// A chip whose songs mostly sit under a chip already picked adds nothing.
const MAX_OVERLAP: f32 = 0.8;
const VERSION: u32 = 2;

/// One descriptor of a song.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Concept {
    pub uri: String,
    pub term: String,
    pub weight: f32,
    /// Descriptor types: 1 genre, 2 mood, 3 activity, and so on.
    #[serde(default)]
    pub types: Vec<u32>,
}

/// The descriptors read so far, by song URI. A song Spotify has nothing for
/// keeps an empty list, so it is not asked about again.
#[derive(Default, Serialize, Deserialize)]
struct DiskCache {
    version: u32,
    tracks: HashMap<String, Vec<Concept>>,
}

pub type Tags = HashMap<String, Arc<[Concept]>>;

/// The in-memory copy of the disk cache, read on first use.
static CACHE: tokio::sync::Mutex<Option<DiskCache>> = tokio::sync::Mutex::const_new(None);

pub fn cache_file(dirs: &crate::paths::AppDirs) -> PathBuf {
    dirs.cache.join("liked-songs").join("descriptors.json")
}

async fn read(path: &Path) -> DiskCache {
    let Ok(bytes) = tokio::fs::read(path).await else {
        return DiskCache::default();
    };
    serde_json::from_slice::<DiskCache>(&bytes)
        .ok()
        .filter(|cache| cache.version == VERSION)
        .unwrap_or_default()
}

async fn write(path: &Path, cache: &DiskCache) {
    let Ok(bytes) = serde_json::to_vec(cache) else {
        return;
    };
    let path = path.to_owned();
    let written = tokio::task::spawn_blocking(move || fastframe_file::write(&path, bytes)).await;
    if let Ok(Err(error)) = written {
        log::warn!("unable to store Liked Songs tags: {error}");
    }
}

/// Answers the songs `uris` from the disk cache, then asks Spotify about the
/// rest batch by batch. `answer` gets every batch as it is known.
pub async fn load(
    session: &Session,
    path: &Path,
    uris: Vec<String>,
    mut answer: impl FnMut(Tags),
) -> anyhow::Result<()> {
    let mut guard = CACHE.lock().await;
    if guard.is_none() {
        *guard = Some(read(path).await);
    }
    let cache = guard.as_mut().expect("read above");
    cache.version = VERSION;

    let mut known = Tags::new();
    let mut missing = Vec::new();
    for uri in uris {
        match cache.tracks.get(&uri) {
            Some(concepts) => {
                known.insert(uri, Arc::from(concepts.as_slice()));
            }
            None if uri.starts_with("spotify:track:") => missing.push(uri),
            None => {}
        }
    }
    if !known.is_empty() {
        answer(known);
    }
    for batch in missing.chunks(BATCH) {
        let fetched = fetch(session, batch).await?;
        let mut tags = Tags::new();
        for uri in batch {
            let concepts = fetched.get(uri).cloned().unwrap_or_default();
            tags.insert(uri.clone(), Arc::from(concepts.as_slice()));
            cache.tracks.insert(uri.clone(), concepts);
        }
        write(path, cache).await;
        answer(tags);
    }
    Ok(())
}

/// Every song's descriptors through extended metadata.
async fn fetch(session: &Session, uris: &[String]) -> anyhow::Result<HashMap<String, Vec<Concept>>> {
    let mut request = BatchedEntityRequest::new();
    for uri in uris {
        request.entity_request.push(EntityRequest {
            entity_uri: uri.clone(),
            query: vec![ExtensionQuery {
                extension_kind: EnumOrUnknown::new(ExtensionKind::TRACK_DESCRIPTOR),
                ..Default::default()
            }],
            ..Default::default()
        });
    }
    let response = session.spclient().get_extended_metadata(request).await?;
    let mut out = HashMap::new();
    for array in &response.extended_metadata {
        if array.extension_kind.enum_value() != Ok(ExtensionKind::TRACK_DESCRIPTOR) {
            continue;
        }
        for data in &array.extension_data {
            let Some(any) = data.extension_data.as_ref() else {
                continue;
            };
            if !matches!(data.header.status_code, 0 | 200) {
                continue;
            }
            out.insert(data.entity_uri.clone(), decode(&any.value));
        }
    }
    Ok(out)
}

/// `ExtensionDescriptorData`: repeated `ExtensionDescriptor descriptors = 1`,
/// each `{ text = 1, weight = 2 (float), types = 3, concept_uri = 4,
/// localized_term = 5 }`.
pub fn decode(bytes: &[u8]) -> Vec<Concept> {
    let mut out = Vec::new();
    for (number, value) in fields(bytes) {
        let (1, Field::Bytes(raw)) = (number, value) else {
            continue;
        };
        let (mut text, mut term, mut uri, mut weight) = (None, None, None, 0.0);
        let mut types = Vec::new();
        for (number, value) in fields(raw) {
            match (number, value) {
                (1, Field::Bytes(raw)) => text = std::str::from_utf8(raw).ok(),
                (2, Field::Fixed32(bits)) => weight = f32::from_bits(bits),
                (3, Field::Bytes(raw)) => {
                    let mut at = 0;
                    while let Some(kind) = varint(raw, &mut at) {
                        types.push(kind as u32);
                    }
                }
                (3, Field::Varint(kind)) => types.push(kind as u32),
                (4, Field::Bytes(raw)) => uri = std::str::from_utf8(raw).ok(),
                (5, Field::Bytes(raw)) => term = std::str::from_utf8(raw).ok(),
                _ => {}
            }
        }
        if let Some(uri) = uri.filter(|uri| !uri.is_empty()) {
            out.push(Concept {
                uri: uri.to_owned(),
                term: term.or(text).unwrap_or(uri).to_owned(),
                weight,
                types,
            });
        }
    }
    out
}

enum Field<'a> {
    Varint(u64),
    Fixed32(u32),
    Bytes(&'a [u8]),
}

fn varint(bytes: &[u8], at: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *bytes.get(*at)?;
        *at += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

/// The fields of one message; stops at the first malformed one.
fn fields(bytes: &[u8]) -> Vec<(u64, Field<'_>)> {
    let mut at = 0;
    let mut out = Vec::new();
    while at < bytes.len() {
        let Some(key) = varint(bytes, &mut at) else {
            break;
        };
        let value = match key & 7 {
            0 => match varint(bytes, &mut at) {
                Some(value) => Field::Varint(value),
                None => break,
            },
            1 => {
                at += 8;
                Field::Varint(0)
            }
            2 => {
                let Some(len) = varint(bytes, &mut at) else {
                    break;
                };
                let Some(raw) = at
                    .checked_add(len as usize)
                    .and_then(|end| bytes.get(at..end))
                else {
                    break;
                };
                at += len as usize;
                Field::Bytes(raw)
            }
            5 => {
                let Some(raw) = bytes.get(at..at + 4) else {
                    break;
                };
                at += 4;
                Field::Fixed32(u32::from_le_bytes(raw.try_into().expect("four bytes")))
            }
            _ => break,
        };
        out.push((key >> 3, value));
    }
    out
}

/// One chip above Liked Songs.
#[derive(Clone, Debug, PartialEq)]
pub struct Chip {
    pub uri: String,
    pub term: String,
    pub songs: usize,
}

/// Whether the song's descriptors carry the concept strongly enough.
pub fn tagged(concepts: &[Concept], concept: &str) -> bool {
    concepts
        .iter()
        .any(|c| c.uri == concept && c.weight >= MIN_WEIGHT)
}

/// The chips for the liked songs `uris`: the concepts that tag the most songs,
/// leaving out one whose songs mostly already sit under a stronger chip of the
/// same type, so Rap does not come with Hip Hop and Pop Rap beside it, while
/// a mood such as Chill still shows next to Trap.
pub fn chips<'a>(uris: impl Iterator<Item = &'a str>, tags: &Tags) -> Vec<Chip> {
    let mut songs: HashMap<&str, HashSet<usize>> = HashMap::new();
    let mut terms: HashMap<&str, &str> = HashMap::new();
    let mut kinds: HashMap<&str, &[u32]> = HashMap::new();
    let mut tagged_songs = 0;
    for (index, uri) in uris.enumerate() {
        let Some(concepts) = tags.get(uri) else {
            continue;
        };
        tagged_songs += 1;
        for concept in concepts.iter().filter(|c| c.weight >= MIN_WEIGHT) {
            songs.entry(&concept.uri).or_default().insert(index);
            terms.entry(&concept.uri).or_insert(&concept.term);
            kinds.entry(&concept.uri).or_insert(&concept.types);
        }
    }
    if tagged_songs < MIN_SONGS {
        return Vec::new();
    }
    let min_songs = (tagged_songs / 100).max(3);
    let mut ranked: Vec<(&str, &HashSet<usize>)> = songs
        .iter()
        .map(|(uri, set)| (*uri, set))
        .filter(|(_, set)| set.len() >= min_songs)
        .collect();
    ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(b.0)));

    let mut picked: Vec<(&str, &HashSet<usize>)> = Vec::new();
    for (uri, set) in ranked {
        if picked.len() == MAX_CHIPS {
            break;
        }
        let kind = kinds.get(uri).copied().unwrap_or_default();
        let redundant = picked.iter().any(|(other, chosen)| {
            let other_kind = kinds.get(other).copied().unwrap_or_default();
            let same_kind = kind.is_empty()
                || other_kind.is_empty()
                || kind.iter().any(|k| other_kind.contains(k));
            same_kind
                && set.intersection(chosen).count() as f32 / set.len() as f32 >= MAX_OVERLAP
        });
        if !redundant {
            picked.push((uri, set));
        }
    }
    picked
        .into_iter()
        .map(|(uri, set)| Chip {
            uri: uri.to_owned(),
            term: title_case(terms.get(uri).copied().unwrap_or(uri)),
            songs: set.len(),
        })
        .collect()
}

fn title_case(term: &str) -> String {
    let mut chars = term.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A song title with its version notes taken off, for telling one song
/// liked twice: "Song (feat. X) - Remastered 2011" reads as "song".
fn plain_title(name: &str) -> String {
    let mut title = name.to_lowercase();
    for cut in [" - ", " (", " ["] {
        if let Some(at) = title.find(cut) {
            title.truncate(at);
        }
    }
    title.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Songs liked more than once under different URIs, as the same recording on
/// an album and on its single: the same ISRC, or the same plain title and
/// first artist with lengths within two seconds. Returns the indices of
/// every such song, each copy right after the first, in list order.
pub fn duplicates(tracks: &[&crate::api::models::Track]) -> Vec<usize> {
    // Union-find over the songs, joined by either key.
    let mut parent: Vec<usize> = (0..tracks.len()).collect();
    fn root(parent: &mut [usize], mut at: usize) -> usize {
        while parent[at] != at {
            parent[at] = parent[parent[at]];
            at = parent[at];
        }
        at
    }
    let mut by_isrc: HashMap<&str, usize> = HashMap::new();
    let mut by_title: HashMap<(String, String), Vec<usize>> = HashMap::new();
    for (index, track) in tracks.iter().enumerate() {
        if track.uri.is_empty() || track.is_local {
            continue;
        }
        if let Some(isrc) = track.external_ids.isrc.as_deref().filter(|isrc| !isrc.is_empty()) {
            if let Some(&first) = by_isrc.get(isrc) {
                let (a, b) = (root(&mut parent, first), root(&mut parent, index));
                parent[b] = a;
            } else {
                by_isrc.insert(isrc, index);
            }
        }
        let artist = track
            .artists
            .first()
            .map(|artist| artist.name.to_lowercase())
            .unwrap_or_default();
        let same = by_title.entry((plain_title(&track.name), artist)).or_default();
        for &other in same.iter() {
            if tracks[other].duration_ms.abs_diff(track.duration_ms) <= 2000 {
                let (a, b) = (root(&mut parent, other), root(&mut parent, index));
                parent[b] = a;
            }
        }
        same.push(index);
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for index in 0..tracks.len() {
        let group = root(&mut parent, index);
        groups.entry(group).or_default().push(index);
    }
    let mut out: Vec<Vec<usize>> = groups
        .into_values()
        .filter(|group| {
            group.len() > 1 && {
                let first = &tracks[group[0]].uri;
                group.iter().any(|&i| &tracks[i].uri != first)
            }
        })
        .collect();
    out.sort_by_key(|group| group[0]);
    out.into_iter().flatten().collect()
}

/// What the app knows about song tags, and each list's chips.
#[derive(Default)]
pub struct State {
    pub tags: Tags,
    /// Songs already asked about, answered or not.
    pub asked: HashSet<String>,
    /// Changes whenever `tags` does.
    pub revision: u64,
    /// The chosen chips' concept URIs, by list ("liked" or a playlist id).
    /// A song shows when it carries every one of them.
    pub selected: HashMap<String, Vec<String>>,
    /// The list revision each list's songs were last checked at.
    pub checked: HashMap<String, (u64, u64)>,
    /// Each list's chips with the revisions they were picked for.
    pub chips: HashMap<String, ((u64, u64), Arc<[Chip]>)>,
    /// Whether Liked Songs shows only the songs liked more than once.
    pub duplicates: bool,
}

impl State {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn receive(&mut self, tags: Tags) {
        self.tags.extend(tags);
        self.revision = self.revision.wrapping_add(1);
    }

    /// Whether the song carries every chosen concept.
    pub fn matches(&self, uri: &str, selected: &[String]) -> bool {
        self.tags
            .get(uri)
            .is_some_and(|concepts| selected.iter().all(|c| tagged(concepts, c)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn concept(uri: &str, weight: f32, kind: u32) -> Concept {
        Concept {
            uri: uri.into(),
            term: uri.into(),
            weight,
            types: vec![kind],
        }
    }

    #[test]
    fn decodes_a_descriptor() {
        // ExtensionDescriptor { text "rap", weight 0.94, types [1], uri, term "Rap" }
        let mut inner = Vec::new();
        inner.extend([0x0a, 3]);
        inner.extend(b"rap");
        inner.push(0x15);
        inner.extend(0.94f32.to_le_bytes());
        inner.extend([0x1a, 1, 1]);
        let uri = b"spotify:concept:4brb6nnsHKHEjyTJhGX4fI";
        inner.extend([0x22, uri.len() as u8]);
        inner.extend(uri);
        inner.extend([0x2a, 3]);
        inner.extend(b"Rap");
        let mut outer = vec![0x0a, inner.len() as u8];
        outer.extend(inner);

        let concepts = decode(&outer);
        assert_eq!(concepts.len(), 1);
        assert_eq!(concepts[0].term, "Rap");
        assert_eq!(concepts[0].uri, "spotify:concept:4brb6nnsHKHEjyTJhGX4fI");
        assert!((concepts[0].weight - 0.94).abs() < 1e-6);
        assert_eq!(concepts[0].types, [1]);
    }

    #[test]
    fn chips_skip_a_concept_inside_a_bigger_one() {
        let mut tags = Tags::new();
        let uris: Vec<String> = (0..40).map(|n| format!("spotify:track:{n}")).collect();
        for (n, uri) in uris.iter().enumerate() {
            let mut concepts = vec![concept("rap", 0.9, 1), concept("hiphop", 0.8, 1)];
            if n % 2 == 0 {
                concepts.push(concept("chill", 0.6, 2));
            }
            if n % 3 == 0 {
                concepts.push(concept("weak", 0.2, 1));
            }
            tags.insert(uri.clone(), Arc::from(concepts));
        }
        let chips = chips(uris.iter().map(String::as_str), &tags);
        let names: Vec<_> = chips.iter().map(|chip| chip.uri.as_str()).collect();
        assert_eq!(names, ["hiphop", "chill"]);
    }

    #[test]
    fn finds_a_song_liked_from_album_and_single() {
        use crate::api::models::{ArtistRef, ExternalIds, Track};
        let track = |uri: &str, name: &str, isrc: Option<&str>, ms: u32| Track {
            uri: uri.into(),
            name: name.into(),
            duration_ms: ms,
            artists: vec![ArtistRef {
                name: "Offset".into(),
                ..Default::default()
            }],
            external_ids: ExternalIds {
                isrc: isrc.map(Into::into),
            },
            ..Default::default()
        };
        let list = [
            track("spotify:track:a", "My Heart", None, 191_000),
            track("spotify:track:b", "Other", Some("US1"), 100_000),
            track("spotify:track:c", "My Heart (feat. X)", None, 190_200),
            track("spotify:track:d", "Different", Some("US1"), 120_000),
            track("spotify:track:e", "My Heart", None, 240_000),
        ];
        let refs: Vec<&Track> = list.iter().collect();
        assert_eq!(duplicates(&refs), [0, 2, 1, 3]);
    }

    #[test]
    fn no_chips_for_a_short_list() {
        let mut tags = Tags::new();
        tags.insert("a".into(), Arc::from(vec![concept("rap", 0.9, 1)]));
        assert!(chips(["a"].into_iter(), &tags).is_empty());
    }
}

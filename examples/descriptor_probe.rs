//! Diagnostic: what the track descriptor extensions return for Liked Songs.
//!
//! Spotify's desktop client builds the genre and mood chips above Liked Songs
//! on the device: it reads each song's descriptors (concept URI, localized
//! term, weight, type such as GENRE or MOOD) and keeps the strongest concepts.
//! This probe asks extended metadata for TRACK_DESCRIPTOR (6) and
//! TRACK_EXTRA_DESCRIPTORS (94) on the latest liked songs and prints what
//! comes back, decoded field by field, plus a tally of the concepts.
//!
//!   cargo run --example descriptor_probe
//!   cargo run --example descriptor_probe -- spotify:track:ID [spotify:track:ID ...]

use std::collections::HashMap;

use librespot_core::{Session, SessionConfig, cache::Cache};
use librespot_protocol::extended_metadata::{BatchedEntityRequest, EntityRequest, ExtensionQuery};
use librespot_protocol::extension_kind::ExtensionKind;
use protobuf::EnumOrUnknown;

const KINDS: [(ExtensionKind, &str); 2] = [
    (ExtensionKind::TRACK_DESCRIPTOR, "TRACK_DESCRIPTOR"),
    (ExtensionKind::TRACK_EXTRA_DESCRIPTORS, "TRACK_EXTRA_DESCRIPTORS"),
];

fn main() -> anyhow::Result<()> {
    fastframe_log::Logging::new("spotifast", env!("CARGO_PKG_VERSION"))
        .filter("warn")
        .init()?;
    let dirs = spotifast::paths::AppDirs::discover().dirs;
    let cache = Cache::new::<&std::path::Path>(None, None, None, None)?.with_memory_credentials();
    let args: Vec<String> = std::env::args().skip(1).collect();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let store = spotifast::credentials::Store::new(dirs);
        let loaded = store
            .lease(spotifast::credentials::Slot::Playback)
            .load()
            .await?;
        if let Some(warning) = loaded.warning {
            eprintln!("{warning}");
        }
        let Some(spotifast::credentials::Grant::Playback(credentials)) = loaded.grant else {
            anyhow::bail!("Enable playback in Spotifast first");
        };
        let session = Session::new(SessionConfig::default(), Some(cache));
        session.connect(credentials, false).await?;
        println!("connected as {}", session.username());

        let uris = if args.is_empty() {
            liked_uris(&session).await?
        } else {
            args
        };
        println!("probing {} songs", uris.len());

        for (kind, name) in KINDS {
            println!("\n===== {name} ({}) =====", kind as i32);
            let mut request = BatchedEntityRequest::new();
            for uri in &uris {
                request.entity_request.push(EntityRequest {
                    entity_uri: uri.clone(),
                    query: vec![ExtensionQuery {
                        extension_kind: EnumOrUnknown::new(kind),
                        ..Default::default()
                    }],
                    ..Default::default()
                });
            }
            let response = match session.spclient().get_extended_metadata(request).await {
                Ok(response) => response,
                Err(error) => {
                    println!("request failed: {error}");
                    continue;
                }
            };
            let mut tally: HashMap<String, (u32, f64)> = HashMap::new();
            let mut shown = 0;
            for array in &response.extended_metadata {
                println!(
                    "array kind={:?} provider_status={}",
                    array.extension_kind, array.header.provider_error_status
                );
                for data in &array.extension_data {
                    let Some(any) = data.extension_data.as_ref() else {
                        println!("  {} status={} (empty)", data.entity_uri, data.header.status_code);
                        continue;
                    };
                    if shown < 5 {
                        println!(
                            "  {} status={} type_url={} bytes={}",
                            data.entity_uri,
                            data.header.status_code,
                            any.type_url,
                            any.value.len()
                        );
                        dump(&any.value, 2);
                        shown += 1;
                    }
                    collect_concepts(&any.value, &mut tally);
                }
            }
            let mut ranked: Vec<_> = tally.into_iter().collect();
            ranked.sort_by(|a, b| b.1.0.cmp(&a.1.0));
            println!("\n  concepts across all songs (songs, summed weight):");
            for (term, (count, weight)) in ranked.iter().take(40) {
                println!("    {count:>4}  {weight:>8.3}  {term}");
            }
        }
        anyhow::Ok(())
    })?;
    Ok(())
}

/// The newest liked songs through Pathfinder's `fetchLibraryTracks`, the
/// query the web player uses, with the session's own token.
async fn liked_uris(session: &Session) -> anyhow::Result<Vec<String>> {
    let token = session.login5().auth_token().await?;
    let client_token = session.spclient().client_token().await?;
    let query = serde_json::json!({
        "operationName": "fetchLibraryTracks",
        "variables": {"offset": 0, "limit": 50},
        "extensions": {"persistedQuery": {
            "version": 1,
            "sha256Hash": "087278b20b743578a6262c2b0b4bcd20d879c503cc359a2285baf083ef944240"
        }}
    });
    let body: serde_json::Value = reqwest::Client::new()
        .post("https://api-partner.spotify.com/pathfinder/v2/query")
        .bearer_auth(&token.access_token)
        .header("client-token", client_token)
        .json(&query)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let uris: Vec<String> = body
        .pointer("/data/me/library/tracks/items")
        .and_then(|items| items.as_array())
        .into_iter()
        .flatten()
        .filter_map(|item| {
            item.pointer("/track/_uri")
                .or_else(|| item.pointer("/track/uri"))
                .or_else(|| item.pointer("/track/data/uri"))
                .and_then(|uri| uri.as_str())
                .map(str::to_owned)
        })
        .collect();
    if uris.is_empty() {
        let text = body.to_string();
        println!("fetchLibraryTracks answer: {}", &text[..text.len().min(1500)]);
    }
    anyhow::ensure!(
        !uris.is_empty(),
        "no liked songs came back; pass spotify:track:… URIs as arguments"
    );
    Ok(uris)
}

/// One protobuf field as read from the wire.
enum Value<'a> {
    Varint(u64),
    Fixed64(u64),
    Bytes(&'a [u8]),
    Fixed32(u32),
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

/// Every field of a message, or None when the bytes are not a message.
fn fields(bytes: &[u8]) -> Option<Vec<(u64, Value<'_>)>> {
    let mut at = 0;
    let mut out = Vec::new();
    while at < bytes.len() {
        let key = varint(bytes, &mut at)?;
        let number = key >> 3;
        if number == 0 {
            return None;
        }
        let value = match key & 7 {
            0 => Value::Varint(varint(bytes, &mut at)?),
            1 => {
                let raw = bytes.get(at..at + 8)?;
                at += 8;
                Value::Fixed64(u64::from_le_bytes(raw.try_into().ok()?))
            }
            2 => {
                let len = varint(bytes, &mut at)? as usize;
                let raw = bytes.get(at..at.checked_add(len)?)?;
                at += len;
                Value::Bytes(raw)
            }
            5 => {
                let raw = bytes.get(at..at + 4)?;
                at += 4;
                Value::Fixed32(u32::from_le_bytes(raw.try_into().ok()?))
            }
            _ => return None,
        };
        out.push((number, value));
    }
    Some(out)
}

fn printable(bytes: &[u8]) -> Option<&str> {
    let text = std::str::from_utf8(bytes).ok()?;
    (!text.is_empty() && text.chars().all(|c| !c.is_control())).then_some(text)
}

fn dump(bytes: &[u8], depth: usize) {
    let pad = "  ".repeat(depth);
    let Some(list) = fields(bytes) else {
        println!("{pad}(not a message: {} bytes)", bytes.len());
        return;
    };
    for (number, value) in list {
        match value {
            Value::Varint(v) => println!("{pad}#{number} varint {v}"),
            Value::Fixed64(v) => println!("{pad}#{number} f64 {}", f64::from_bits(v)),
            Value::Fixed32(v) => println!("{pad}#{number} f32 {}", f32::from_bits(v)),
            Value::Bytes(raw) => {
                if let Some(text) = printable(raw) {
                    println!("{pad}#{number} \"{text}\"");
                } else if fields(raw).is_some() {
                    println!("{pad}#{number} {{");
                    dump(raw, depth + 1);
                    println!("{pad}}}");
                } else {
                    println!("{pad}#{number} bytes[{}]", raw.len());
                }
            }
        }
    }
}

/// Tallies every nested message that carries a `spotify:concept:` URI, keyed
/// by its readable term when one sits beside it, with any float as weight.
fn collect_concepts(bytes: &[u8], tally: &mut HashMap<String, (u32, f64)>) {
    let Some(list) = fields(bytes) else { return };
    let mut concept = None;
    let mut term = None;
    let mut weight = 0.0;
    for (_, value) in &list {
        match value {
            Value::Bytes(raw) => match printable(raw) {
                Some(text) if text.starts_with("spotify:concept:") => concept = Some(text),
                Some(text) => term = term.or(Some(text)),
                None => collect_concepts(raw, tally),
            },
            Value::Fixed32(v) => weight = f64::from(f32::from_bits(*v)),
            Value::Fixed64(v) => weight = f64::from_bits(*v),
            Value::Varint(_) => {}
        }
    }
    if let Some(concept) = concept {
        let key = match term {
            Some(term) => format!("{term}  [{concept}]"),
            None => concept.to_owned(),
        };
        let entry = tally.entry(key).or_default();
        entry.0 += 1;
        entry.1 += weight;
    }
}

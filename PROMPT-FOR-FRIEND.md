Сделай в форке Spotifast (Rust + egui, https://github.com/crmne/spotifast) жанровые чипы в Liked Songs, как в десктопном Spotify, и пару правок интерфейса. Работай в отдельной ветке. Ниже всё, что уже выяснено, — заново не исследуй.

## Откуда брать теги

Отдельного эндпоинта «теги лайков» нет: десктопный Spotify считает чипы на клиенте из описаний каждого трека.

- Описания отдаёт extended metadata: `ExtensionKind::TRACK_DESCRIPTOR` (= 6) через `session.spclient().get_extended_metadata(BatchedEntityRequest)`. Spotifast уже делает такие запросы в `src/session_reads.rs` (PRERELEASE, ALBUM_V4) — делай так же. Пачки по 200 треков работают.
- Ответ: `type.googleapis.com/spotify.descriptorextension.ExtensionDescriptorData`. Сгенерированного proto в librespot нет, разбирай wire-формат руками:
  - `ExtensionDescriptorData { repeated ExtensionDescriptor descriptors = 1; }`
  - `ExtensionDescriptor { string text = 1; float weight = 2; repeated enum types = 3 (packed); string concept_uri = 4; string localized_term = 5; }`
  - types: 1 GENRE, 2 MOOD, 3 ACTIVITY, …
  - Пример: `"trap", 0.92, [1], "spotify:concept:1F8T8XLbH93QsW2CcsCeM9", "Trap"`.
- `TRACK_EXTRA_DESCRIPTORS` (94) возвращает пусто, он не нужен.
- Список лайков через Web API `/v1/me/tracks` у меня давал 429. Если нужны URI вне приложения, бери Pathfinder `fetchLibraryTracks`, хэш `087278b20b743578a6262c2b0b4bcd20d879c503cc359a2285baf083ef944240`, переменные `{offset, limit}`, ответ в `data.me.library.tracks.items[].track._uri`. В самом приложении URI уже лежат в `app.library.liked.items`.

## Что сделать

1. Новый модуль `src/liked_tags.rs`:
   - декодер описаний;
   - кэш на диске `cache/liked-songs/descriptors.json` по URI трека (трек без описаний хранится с пустым списком, чтобы не спрашивать снова);
   - `load()`: сначала отвечает из кэша, остальное докачивает пачками и пишет кэш;
   - `chips()`: считает, у скольких лайков есть концепт с весом ≥ 0.5. Нет чипов, если треков с тегами меньше 30. Порог — не меньше max(3, 1% треков). Сортировка по числу треков, максимум 15. Отсев дублей: концепт пропускается, если ≥ 80% его треков уже покрыты выбранным чипом **того же типа** (Rap без Hip Hop и Pop Rap рядом, но Chill рядом с Trap остаётся).
   - Тесты на декодер и выбор чипов.
2. Бэкенд (`src/backend.rs`): `Command::LikedTags { uris, generation }` и `Event::LikedTags { generation, tags }`, по образцу `fetch_friend_activity` / `fetch_play_counts`. Ответ отправлять после каждой пачки.
3. App (`src/app.rs`): поле `liked_tags` рядом с `play_count_reads`, сброс вместе с ними при смене аккаунта (генерация `play_count_generation`). `request_liked_tags()` при каждой смене `library.liked.revision` отправляет только URI, которых ещё не спрашивали, — так новый лайк получит теги сам.
4. UI (`src/ui/collection.rs`, функция `liked`):
   - Пока открыт Liked Songs, догружать список до конца (`Action::LoadMore(Page::LikedSongs)`).
   - Ряд чипов под рядом с кнопкой Play (`theme::soft_button`). Клик — фильтр, повторный клик — сброс.
   - Фильтровать `items` до `prepare_table_view`, ставить `plays_shown_rows: true` и отдельный `items_revision` (хэш ревизии лайков, ревизии тегов и выбранного концепта), чтобы Play играл только отфильтрованное и кэши таблицы не путались.
5. Стрелки у ряда чипов (`src/ui/widgets.rs`, новая `chip_row_with_arrows` рядом с `chip_row`): белые круглые кнопки с `Icon::ChevronLeft/Right` по краям и градиент. Правая видна, пока справа есть скрытые чипы, левая — когда ряд прокручен. Клик сдвигает на 70% ширины через `ScrollArea::scroll_offset`. Вертикальный центр стрелок бери по `content_size.y`, а не по `inner_rect`: иначе они уезжают на строку таблицы.
6. Кнопка лайка как в Spotify — в плеере (`src/ui/player_bar.rs`) и в строках таблицы (`widgets.rs`, колонка heart):
   - не сохранён: `Icon::CirclePlus`, клик сразу кладёт в Liked Songs;
   - сохранён: залитый зелёный круг (`palette.accent`) с чёрной `Icon::Check`, клик открывает чек-лист «Liked Songs + плейлисты» с кнопкой Done.
   - Этот чек-лист в плеере уже есть. Вынеси его в общую функцию `widgets::playlist_checklist(ui, app, &response, popup_id, item)` и вызывай из плеера и из строк (id попапа на строку: `("row-liked-playlists", uri)`).
7. По желанию — оригинальная иконка Spotify: вытащить 256/48/32/16 px из `%APPDATA%\Spotify\Spotify.exe` (Win32 `PrivateExtractIcons`), собрать `packaging/windows/spotifast.ico`. В `util::app_icon_rgba` декодировать встроенный PNG 256 и ресайзить (`image::imageops::resize`, Lanczos3). Старый `mark_rgba` оставить запасным вариантом, а тест полированной иконки перевести на него. Только для себя: это товарный знак Spotify.

## Сборка на Windows

Rust ставится через rustup, версию 1.98 подтянет `rust-toolchain.toml`. Нужны VS Build Tools. Без MilkDrop (иначе нужен vcpkg):

    cargo build --locked --no-default-features
    cargo test --locked --no-default-features --lib liked_tags

Работающий `spotifast.exe` нельзя перезаписать: перед сборкой переименуй его или закрой приложение. При убийстве процесса Soloist продолжает играть сам.

# Fork: Spotify desktop look, measured

Measured from open.spotify.com (same layout as the desktop app) at a
1440x900 viewport with `getComputedStyle`. Spotify's own font (Spotify Mix)
is licensed to Spotify and is not copied; the fork keeps Inter. Icons are the
closest Lucide shapes, not Spotify's.

## Colours

| Token | Value |
|---|---|
| window (gaps between panels) | `#000000` |
| panel (sidebar, main, right) | `#121212` |
| surface (search field, home button, cards hover) | `#1f1f1f` |
| row hover | `#2a2a2a` (rgba 255/0.07 over #121212) |
| chip | `rgba(255,255,255,0.1)`, text `#ffffff` |
| chip active | `#ffffff`, text `#000000` |
| text | `#ffffff` |
| secondary text | `#b3b3b3` |
| outline button border | `#7c7c7c` 1px |
| accent | `#1ed760`, hover `#3be477`, on accent `#000000` |
| progress track | `rgba(255,255,255,0.3)`, 4px tall, radius 2 |

## Shell

- Top bar 64 tall. Home button 48x48 round on `#1f1f1f`; search field 48 tall,
  pill, `#1f1f1f`, 474 wide, text inset 48 left.
- Panels start at y=64, 8px gutters on every side, radius 8.
- Sidebar default 331 wide (x=8). Main from x=347.
- Player bar 72 tall (66 logged out), below the panels with 8px gutter.
  Play/pause 32 round white, transport icons 16, progress 4px.

## Page header (playlist, album)

- Header area padding 0 24, height ~336 with a colour wash.
- Kind label 14px regular white ("Public Playlist").
- Title 96px weight 800 white, shrinks to fit.
- Description 14px, byline 14px; owner 14px bold.

## Action bar

- Play 56 round accent, black triangle.
- Gap 24 between controls. Shuffle 32, save 32, more 32 (icons ~24-32).
- Follow: outline pill 32 tall, padding 4 16, 14px bold, white text,
  1px `#7c7c7c` border.
- List/view toggle right: 14px `rgba(255,255,255,0.7)`.

## Track list

- Column header row 36 tall, 14px `#b3b3b3`, sentence case ("Title",
  "Album", "Date added"), clock icon for duration; thin divider under it.
- Row 56 tall, padding 0 16, gap 16, radius 4, hover `#2a2a2a`.
- Number column 16 wide, 16px `#b3b3b3`.
- Cover 40x40 radius 4.
- Title 16px white; artist 14px `#b3b3b3`.
- Columns at 1440: # 16, title 381, album 190, date 190, duration 120.

## Sections and cards

- Section title 24px bold white; "Show all" 14px bold `#b3b3b3`.
- Card 196 wide incl. padding 12, image 172 (artist page 180), radius 6,
  gap 8 under the image; hover fill `#1f1f1f`.

## Chips

- 32 tall, padding 4 12, pill, 14px regular. Inactive `rgba(255,255,255,0.1)`
  white text; active white with black text.

## Artist page

- Banner ~341 tall across the main panel; name 96px/800; monthly listeners
  16px white under it; verified mark above the name.
- Action row: Play 56, Shuffle 32, Follow pill, More.
- Popular: rows as the track list, columns # / title / plays / duration.

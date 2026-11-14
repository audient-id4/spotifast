Unofficial fork of [Spotifast](https://github.com/crmne/spotifast) with a Spotify-desktop-style interface and extra Liked Songs tools: genre and mood chips (several at once, saved as a playlist), the save-to-playlists checklist, a duplicates finder, "You Liked" on artist pages, and a stats page from your Spotify data export.

## Download

| System | File |
| --- | --- |
| Windows 10/11 (x64) | `spotifast-…-windows-x64.zip`, unzip and run `spotifast.exe` |
| macOS 12+ (Apple Silicon and Intel) | `spotifast-…-macos-universal.dmg`, drag Spotifast to Applications |
| Linux x86_64 | `spotifast-…-linux-x86_64.tar.gz` |

**These builds are not signed.**
- Windows: SmartScreen warns once. Choose **More info**, then **Run anyway**.
- macOS: the first launch is blocked. Right-click Spotifast.app and choose **Open**, or allow it under System Settings → Privacy & Security. If macOS says the app "is damaged", run `xattr -dr com.apple.quarantine /Applications/Spotifast.app`.

## Before you sign in

- Playback needs **Spotify Premium**.
- You need your own Client ID from the [Spotify developer dashboard](https://developer.spotify.com/dashboard). Set it up as the README describes ("Sign in"). It takes about two minutes.

The built-in updater is off in this fork, because it would replace the fork with upstream Spotifast. To update, download new releases from this page.

`checksums.txt` lists the SHA-256 of every file.

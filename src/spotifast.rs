//! Spotifast desktop command.

// Fork: no console window on Windows, debug builds included; the log is
// still written to spotifast.log in the state folder.
#![windows_subsystem = "windows"]

mod entrypoint;

fn main() -> eframe::Result<()> {
    entrypoint::run()
}

//! The app's log: the backend and the webview write to one file in the OS log
//! folder, a fresh file per session, with the last few sessions kept beside it.
//! See the Logging section of AGENTS.md for what belongs at which level.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use log::LevelFilter;
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_log::{FileOpenStrategy, RotationStrategy, Target, TargetKind};
use tauri_plugin_opener::OpenerExt;

const LOG_NAME: &str = "open-darkroom";
const MAX_FILE_BYTES: u128 = 5 * 1024 * 1024;
const KEPT_SESSIONS: usize = 10;
const CRATE_TARGET: &str = env!("CARGO_CRATE_NAME");
const MARKER_PREFIX: &str = "session-";
const MARKER_SUFFIX: &str = ".running";

/// Lines of the current session's log a report carries, and of a session that
/// ended without shutting down: together they fit a GitHub issue body.
const REPORT_LINES: usize = 200;
const UNCLEAN_REPORT_LINES: usize = 150;

/// Registers the log plugin and attaches the logger. A log folder that cannot
/// be written leaves the app running without a file rather than not starting.
pub fn init<R: Runtime>(app: &AppHandle<R>) {
    let mut targets = vec![Target::new(TargetKind::LogDir { file_name: Some(LOG_NAME.into()) })];
    if cfg!(debug_assertions) {
        targets.push(Target::new(TargetKind::Stdout));
    }

    let builder = tauri_plugin_log::Builder::new()
        .clear_targets()
        .targets(targets)
        .level(LevelFilter::Debug)
        // Other crates' debug output is noise next to ours; wgpu's info is too.
        .filter(|meta| meta.level() <= log::Level::Info || is_own(meta.target()))
        .level_for("wgpu_core", LevelFilter::Warn)
        .level_for("wgpu_hal", LevelFilter::Warn)
        .level_for("naga", LevelFilter::Warn)
        .max_file_size(MAX_FILE_BYTES)
        .rotation_strategy(RotationStrategy::KeepSome(KEPT_SESSIONS))
        .file_open_strategy(FileOpenStrategy::Rotate)
        .format(|out, message, record| {
            let target = record.target();
            let target = target
                .strip_prefix(CRATE_TARGET)
                .map(|rest| rest.trim_start_matches("::"))
                .filter(|rest| !rest.is_empty())
                .unwrap_or(target);
            out.finish(format_args!(
                "{} {:<5} [{}] {}",
                timestamp(),
                record.level(),
                target,
                redact(&message.to_string()),
            ))
        });

    match builder.split(app) {
        Ok((plugin, max_level, logger)) => {
            if let Err(e) = tauri_plugin_log::attach_logger(max_level, logger) {
                eprintln!("The logger could not be attached: {e}");
            }
            if let Err(e) = app.plugin(plugin) {
                eprintln!("The log plugin could not be registered: {e}");
            }
        }
        Err(e) => eprintln!("Logging is unavailable: {e}"),
    }
    set_verbose(false);
}

fn is_own(target: &str) -> bool {
    target.starts_with(CRATE_TARGET) || target.starts_with("webview") || target == "panic"
}

/// Debug lines are kept only when asked for, outside a debug build.
pub fn set_verbose(verbose: bool) {
    let verbose = verbose || cfg!(debug_assertions);
    log::set_max_level(if verbose { LevelFilter::Debug } else { LevelFilter::Info });
}

fn timestamp() -> String {
    let format = time::macros::format_description!(
        "[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:3]Z"
    );
    time::OffsetDateTime::now_utc().format(&format).unwrap_or_default()
}


// =============== REDACTION ===============

/// The home folder as it can appear in a message, so a log pasted into a
/// public issue does not carry the user's account name.
fn home_forms() -> &'static [String] {
    static HOME: OnceLock<Vec<String>> = OnceLock::new();
    HOME.get_or_init(|| {
        let Some(home) = std::env::home_dir() else { return Vec::new() };
        let home = home.to_string_lossy().trim_end_matches(['/', '\\']).to_string();
        if home.len() < 2 {
            return Vec::new();
        }
        let forward = home.replace('\\', "/");
        if forward == home { vec![home] } else { vec![home, forward] }
    })
}

/// Replaces the home folder with `~` wherever it starts a path.
pub fn redact(text: &str) -> String {
    let mut text = text.to_string();
    for home in home_forms() {
        if text.contains(home.as_str()) {
            text = replace_home(&text, home);
        }
    }
    text
}

fn replace_home(text: &str, home: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(home) {
        let after = &rest[at + home.len()..];
        // `/Users/al` must not eat the start of `/Users/alice`.
        let whole = after
            .chars()
            .next()
            .is_none_or(|c| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.')));
        out.push_str(&rest[..at]);
        out.push_str(if whole { "~" } else { home });
        rest = after;
    }
    out.push_str(rest);
    out
}


// =============== PANICS ===============

/// Logs a panic before anything else happens to the process: where, on which
/// thread, and the backtrace. The default hook still runs after it.
pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("non-string payload");
        let location = info.location().map_or_else(|| "an unknown location".into(), |l| l.to_string());
        let backtrace = std::backtrace::Backtrace::force_capture();
        log::error!(
            target: "panic",
            "thread '{}' panicked at {location}: {payload}\n{backtrace}",
            thread.name().unwrap_or("unnamed"),
        );
        log::logger().flush();
        default_hook(info);
    }));
}


// =============== SESSIONS ===============

/// This session's running marker, and what the markers of sessions that ended
/// without shutting down said. A marker is held locked for the life of the
/// process, so one whose lock can be taken belonged to a process that is gone.
pub struct Session {
    log_dir: PathBuf,
    marker: Mutex<Option<(File, PathBuf)>>,
    unclean: Vec<String>,
}

pub fn begin_session<R: Runtime>(app: &AppHandle<R>) -> Session {
    let info = app.package_info();
    log::info!(
        "{} {} ({} build) on {} {}, WebView {}",
        info.name,
        info.version,
        build_kind(),
        os_info::get(),
        std::env::consts::ARCH,
        tauri::webview_version().unwrap_or_else(|e| format!("unknown ({e})")),
    );

    let log_dir = app.path().app_log_dir().unwrap_or_default();
    let unclean = reap_markers(&log_dir);
    for previous in &unclean {
        log::warn!("A previous session did not shut down cleanly ({previous})");
    }
    let marker = create_marker(&log_dir, &info.version.to_string());
    Session { log_dir, marker: Mutex::new(marker), unclean }
}

pub fn end_session<R: Runtime>(app: &AppHandle<R>) {
    log::info!("Shutting down");
    let Some(session) = app.try_state::<Session>() else { return };
    let marker = session.marker.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some((file, path)) = marker {
        drop(file);
        let _ = fs::remove_file(path);
    }
    log::logger().flush();
}

fn build_kind() -> &'static str {
    if cfg!(debug_assertions) { "debug" } else { "release" }
}

fn reap_markers(log_dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(log_dir) else { return Vec::new() };
    let mut unclean = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !(name.starts_with(MARKER_PREFIX) && name.ends_with(MARKER_SUFFIX)) {
            continue;
        }
        let Ok(mut file) = OpenOptions::new().read(true).write(true).open(&path) else { continue };
        match file.try_lock() {
            Ok(()) => {
                let mut said = String::new();
                let _ = file.read_to_string(&mut said);
                drop(file);
                let _ = fs::remove_file(&path);
                unclean.push(said.trim().to_string());
            }
            // Another copy of the app is running and owns it.
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(e)) => log::debug!("Could not check {name}: {e}"),
        }
    }
    unclean
}

fn create_marker(log_dir: &Path, version: &str) -> Option<(File, PathBuf)> {
    let pid = std::process::id();
    let path = log_dir.join(format!("{MARKER_PREFIX}{pid}{MARKER_SUFFIX}"));
    let created = File::create(&path).and_then(|mut file| {
        file.try_lock().map_err(std::io::Error::from)?;
        write!(file, "version {version}, started {}, pid {pid}", timestamp())?;
        file.flush()?;
        Ok(file)
    });
    match created {
        Ok(file) => Some((file, path)),
        Err(e) => {
            log::warn!("Could not write the session marker, so a crash will go unnoticed: {e}");
            None
        }
    }
}


// =============== COMMANDS ===============

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingStatus {
    folder: String,
    previous_session_unclean: bool,
}

#[tauri::command]
pub(crate) fn logging_status(session: tauri::State<'_, Session>) -> LoggingStatus {
    LoggingStatus {
        folder: redact(&session.log_dir.to_string_lossy()),
        previous_session_unclean: !session.unclean.is_empty(),
    }
}

#[tauri::command]
pub(crate) fn set_verbose_logging(verbose: bool) {
    set_verbose(verbose);
    log::info!("Verbose logging {}", if verbose { "on" } else { "off" });
}

/// Shows the current log file in the system's file manager.
#[tauri::command]
pub(crate) fn reveal_log_file(app: AppHandle, session: tauri::State<'_, Session>) -> Result<(), String> {
    let current = session.log_dir.join(format!("{LOG_NAME}.log"));
    let opened = if current.is_file() {
        app.opener().reveal_item_in_dir(&current)
    } else {
        app.opener().open_path(session.log_dir.to_string_lossy(), None::<&str>)
    };
    opened.map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct Diagnostics {
    summary: String,
    logs: String,
}

/// System details and the recent log, for a bug report. The frontend adds
/// what only the webview knows, such as the GPU adapter.
#[tauri::command]
pub(crate) fn diagnostics(app: AppHandle, session: tauri::State<'_, Session>) -> Diagnostics {
    let info = app.package_info();
    let previous = if session.unclean.is_empty() {
        "ended cleanly".to_string()
    } else {
        format!("did not shut down cleanly ({})", session.unclean.join("; "))
    };
    let summary = [
        format!("{} {} ({} build)", info.name, info.version, build_kind()),
        format!("OS: {} {}", os_info::get(), std::env::consts::ARCH),
        format!(
            "WebView: {}",
            tauri::webview_version().unwrap_or_else(|e| format!("unknown ({e})"))
        ),
        format!("Log folder: {}", redact(&session.log_dir.to_string_lossy())),
        format!("Previous session: {previous}"),
    ]
    .join("\n");

    let mut logs = section(
        &session.log_dir.join(format!("{LOG_NAME}.log")),
        "This session",
        REPORT_LINES,
    );
    if !session.unclean.is_empty() {
        if let Some(previous) = newest_archived_log(&session.log_dir) {
            logs.push_str("\n\n");
            logs.push_str(&section(&previous, "Previous session", UNCLEAN_REPORT_LINES));
        }
    }
    Diagnostics { summary, logs }
}

fn section(path: &Path, title: &str, lines: usize) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    match fs::read(path) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            let all: Vec<&str> = text.lines().collect();
            let kept = &all[all.len().saturating_sub(lines)..];
            format!("--- {title}: {name}, last {} of {} lines ---\n{}", kept.len(), all.len(), kept.join("\n"))
        }
        Err(e) => format!("--- {title}: {name} could not be read: {e} ---"),
    }
}

/// Archived logs are named `<LOG_NAME>_<date>.log`, so the newest sorts last.
fn newest_archived_log(log_dir: &Path) -> Option<PathBuf> {
    let prefix = format!("{LOG_NAME}_");
    fs::read_dir(log_dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .map(|n| n.to_string_lossy())
                .is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".log"))
        })
        .max()
}


#[cfg(test)]
mod tests {
    use super::replace_home;

    #[test]
    fn replaces_the_home_folder_only_where_it_ends() {
        assert_eq!(replace_home("/Users/al/Pictures/a.nef", "/Users/al"), "~/Pictures/a.nef");
        assert_eq!(replace_home("open /Users/al", "/Users/al"), "open ~");
        assert_eq!(replace_home("/Users/alice/a.nef", "/Users/al"), "/Users/alice/a.nef");
        assert_eq!(
            replace_home(r"C:\Users\Al\a.tif and C:\Users\Al\b.tif", r"C:\Users\Al"),
            r"~\a.tif and ~\b.tif"
        );
    }
}

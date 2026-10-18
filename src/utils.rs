//! Shared helpers used by rules and the corrector.
//!
//! Behavior follows `revise.utils`, including `shutil.which`, close-match
//! fallbacks, and the on-disk cache keyed by file modification time.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use crate::conf;
use crate::difflib;
use crate::logs;
use crate::shells;
use crate::types::Command;

thread_local! {
    static WHICH_HOOK: std::cell::Cell<Option<fn(&str) -> Option<String>>> = const { std::cell::Cell::new(None) };
    static EXES_HOOK: std::cell::RefCell<Option<Vec<String>>> = std::cell::RefCell::new(None);
    static HISTORY_HOOK: std::cell::RefCell<Option<Vec<String>>> = std::cell::RefCell::new(None);
    static MEMO_OFF: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static CACHE_OFF: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn set_which_hook(hook: Option<fn(&str) -> Option<String>>) {
    WHICH_HOOK.with(|cell| cell.set(hook));
}

pub fn set_executables_hook(executables: Option<Vec<String>>) {
    EXES_HOOK.with(|cell| *cell.borrow_mut() = executables);
}

pub fn set_history_hook(history: Option<Vec<String>>) {
    HISTORY_HOOK.with(|cell| *cell.borrow_mut() = history);
}

pub fn set_memoize_disabled(disabled: bool) {
    MEMO_OFF.with(|cell| cell.set(disabled));
}

pub fn set_cache_disabled(disabled: bool) {
    CACHE_OFF.with(|cell| cell.set(disabled));
}

pub fn clear_test_hooks() {
    set_which_hook(None);
    set_executables_hook(None);
    set_history_hook(None);
    set_memoize_disabled(false);
    set_cache_disabled(false);
    clear_memo_caches();
}

fn memo_off() -> bool {
    MEMO_OFF.with(|cell| cell.get())
}

fn cache_off() -> bool {
    CACHE_OFF.with(|cell| cell.get())
}

static WHICH_CACHE: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);
static EXES_CACHE: Mutex<Option<Vec<String>>> = Mutex::new(None);

pub fn clear_memo_caches() {
    *WHICH_CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    *EXES_CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    shells::clear_history_cache();
}

pub fn get_alias() -> String {
    std::env::var("REVISE_ALIAS").unwrap_or_else(|_| "revise".to_string())
}

pub fn which(program: &str) -> Option<String> {
    if let Some(hook) = WHICH_HOOK.with(|cell| cell.get()) {
        return hook(program);
    }
    if !memo_off() {
        let mut cache = WHICH_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        let cache = cache.get_or_insert_with(HashMap::new);
        if let Some(hit) = cache.get(program) {
            return hit.clone();
        }
        let found = which_uncached(program);
        cache.insert(program.to_string(), found.clone());
        return found;
    }
    which_uncached(program)
}

fn which_uncached(program: &str) -> Option<String> {
    let (dir, file) = path_split(program);
    let dirs: Vec<PathBuf> = if !dir.is_empty() {
        vec![PathBuf::from(&dir)]
    } else {
        match std::env::var_os("PATH") {
            None => std::env::split_paths(if cfg!(windows) {
                r".;C:\Windows\System32"
            } else {
                "/usr/bin:/bin"
            })
            .collect(),
            Some(path) if path.is_empty() => return None,
            Some(path) => std::env::split_paths(&path).collect(),
        }
    };
    let mut seen = std::collections::HashSet::new();
    for dir in dirs {
        let key = dir.to_string_lossy().to_string();
        if !seen.insert(key) {
            continue;
        }
        let candidate = dir.join(&file);
        if is_executable(&candidate) {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let c = std::ffi::CString::new(path.to_string_lossy().as_bytes()).ok()?;
    unsafe { libc::access(c.as_ptr(), libc::X_OK) == 0 }
}

pub fn get_close_matches(word: &str, possibilities: &[impl AsRef<str>], n: Option<usize>, cutoff: f64) -> Vec<String> {
    let n = n.unwrap_or_else(conf::num_close_matches);
    if n == 0 {
        return Vec::new();
    }
    difflib::get_close_matches(word, possibilities, n, cutoff)
}

pub fn get_closest(
    word: &str,
    possibilities: impl IntoIterator<Item = impl AsRef<str>>,
    cutoff: f64,
    fallback_to_first: bool,
) -> Option<String> {
    let possibilities: Vec<String> = possibilities.into_iter().map(|s| s.as_ref().to_string()).collect();
    let matches = get_close_matches(word, &possibilities, Some(1), cutoff);
    if let Some(best) = matches.into_iter().next() {
        Some(best)
    } else if fallback_to_first {
        possibilities.into_iter().next()
    } else {
        None
    }
}

pub fn replace_argument(script: &str, from: &str, to: &str) -> String {
    let suffix = format!(" {from}");
    if let Some(stripped) = script.strip_suffix(&suffix) {
        return format!("{stripped} {to}");
    }
    let needle = format!(" {from} ");
    let replacement = format!(" {to} ");
    if let Some(index) = script.find(&needle) {
        let mut out = String::with_capacity(script.len() + to.len());
        out.push_str(&script[..index]);
        out.push_str(&replacement);
        out.push_str(&script[index + needle.len()..]);
        out
    } else {
        script.to_string()
    }
}

pub fn get_all_matched_commands(stderr: &str, separators: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let mut should_yield = false;
    for line in stderr.split('\n') {
        if separators.iter().any(|sep| line.contains(sep)) {
            should_yield = true;
            continue;
        }
        if should_yield && !line.is_empty() {
            out.push(line.trim().to_string());
        }
    }
    out
}

pub fn replace_command(command: &Command, broken: &str, matched: &[impl AsRef<str>]) -> Vec<String> {
    get_close_matches(broken, matched, None, 0.1)
        .into_iter()
        .map(|new_cmd| replace_argument(&command.script, broken, new_cmd.trim()))
        .collect()
}

pub fn is_app(command: &Command, app_names: &[&str], at_least: usize) -> bool {
    let parts = command.script_parts();
    if parts.len() > at_least {
        let binary = basename(&parts[0]);
        app_names.iter().any(|name| *name == binary)
    } else {
        false
    }
}

pub fn for_app(command: &Command, app_names: &[&str], at_least: usize) -> bool {
    is_app(command, app_names, at_least)
}

pub fn include_path_in_search(path: &str) -> bool {
    let prefixes = conf::excluded_search_path_prefixes();
    !prefixes.iter().any(|prefix| path.starts_with(prefix))
}

pub fn get_all_executables() -> Vec<String> {
    if let Some(hooked) = EXES_HOOK.with(|cell| cell.borrow().clone()) {
        return hooked;
    }
    if !memo_off() {
        let mut cache = EXES_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(hit) = cache.as_ref() {
            return hit.clone();
        }
        let found = executables_uncached();
        *cache = Some(found.clone());
        return found;
    }
    executables_uncached()
}

fn executables_uncached() -> Vec<String> {
    let revise_alias = get_alias();
    let mut bins = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let dir_text = dir.to_string_lossy().to_string();
            if !include_path_in_search(&dir_text) {
                continue;
            }
            let entries = match fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if is_dir_safe(&path) {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if name == "revise" {
                    continue;
                }
                bins.push(name);
            }
        }
    }
    let aliases = shells::current().get_aliases();
    for name in aliases.keys() {
        if name != &revise_alias {
            bins.push(name.clone());
        }
    }
    bins
}

fn is_dir_safe(path: &Path) -> bool {
    match fs::metadata(path) {
        Ok(meta) => meta.is_dir(),
        Err(_) => true,
    }
}

pub fn get_valid_history_without_current(command: &Command) -> Vec<String> {
    if let Some(hooked) = HISTORY_HOOK.with(|cell| cell.borrow().clone()) {
        return hooked;
    }
    let history = shells::current().get_history();
    let revise_alias = get_alias();
    let mut executables: std::collections::HashSet<String> = get_all_executables().into_iter().collect();
    for builtin in shells::current().builtin_commands() {
        executables.insert(builtin.to_string());
    }
    not_corrected(&history, &revise_alias)
        .into_iter()
        .filter(|line| {
            !line.starts_with(&revise_alias)
                && line != &command.script
                && line.split(' ').next().is_some_and(|bin| executables.contains(bin))
        })
        .collect()
}

/// Every history line except the one that sits immediately before `revise`.
fn not_corrected(history: &[String], revise_alias: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut previous: Option<&str> = None;
    for line in history {
        if previous.is_some() && line != revise_alias {
            out.push(previous.unwrap().to_string());
        }
        previous = Some(line.as_str());
    }
    if let Some(last) = history.last() {
        out.push(last.clone());
    }
    out
}

pub fn format_raw_script(raw: &[String]) -> String {
    raw.join(" ").trim_start().to_string()
}

pub fn cached_json(key: &str, depends_on: &[&str], compute: impl FnOnce() -> serde_json::Value) -> serde_json::Value {
    if cache_off() {
        return compute();
    }
    let depends: Vec<String> = depends_on.iter().map(|path| abspath(&expanduser_path(path).to_string_lossy())).collect();
    let etag = depends
        .iter()
        .map(|path| mtime_string(path))
        .collect::<Vec<_>>()
        .join(".");
    let cache_path = cache_file();
    let mut db = read_cache(&cache_path);
    if let Some(entry) = db.get(key) {
        if entry.get("etag").and_then(|v| v.as_str()) == Some(etag.as_str()) {
            if let Some(value) = entry.get("value") {
                return value.clone();
            }
        }
    }
    let value = compute();
    db.insert(
        key.to_string(),
        serde_json::json!({ "etag": etag, "value": value }),
    );
    write_cache(&cache_path, &db);
    value
}

fn cache_file() -> PathBuf {
    let base = std::env::var("XDG_CACHE_HOME").unwrap_or_else(|_| "~/.cache".to_string());
    let dir = expanduser_path(&base);
    let _ = fs::create_dir_all(&dir);
    dir.join("revise-cache.json")
}

fn read_cache(path: &Path) -> HashMap<String, serde_json::Value> {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write_cache(path: &Path, db: &HashMap<String, serde_json::Value>) {
    if let Ok(bytes) = serde_json::to_vec(db) {
        if fs::write(path, bytes).is_err() {
            logs::debug("Unable to write cache");
        }
    }
}

fn mtime_string(path: &str) -> String {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|dur| format!("{}.{}", dur.as_secs(), dur.subsec_nanos()))
        .unwrap_or_else(|| "0".to_string())
}

pub fn expanduser_path(path: &str) -> PathBuf {
    if path == "~" || path.starts_with("~/") || path.starts_with("~\\") {
        if let Some(home) = home_dir() {
            if path == "~" {
                return home;
            }
            return home.join(&path[2..]);
        }
    } else if let Some(rest) = path.strip_prefix('~') {
        if let Some(name) = rest.split(['/', '\\']).next() {
            if !name.is_empty() {
                if let Some(home) = home_of(name) {
                    let prefix = format!("~{name}");
                    let tail = &path[prefix.len()..];
                    let tail = tail.trim_start_matches(['/', '\\']);
                    return if tail.is_empty() { home } else { home.join(tail) };
                }
            }
        }
    }
    PathBuf::from(path)
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from).or_else(|| {
        let uid = unsafe { libc::getuid() };
        let pw = unsafe { libc::getpwuid(uid) };
        if pw.is_null() {
            return None;
        }
        let dir = unsafe { std::ffi::CStr::from_ptr((*pw).pw_dir) };
        dir.to_str().ok().map(PathBuf::from)
    })
}

fn home_of(name: &str) -> Option<PathBuf> {
    let c = std::ffi::CString::new(name).ok()?;
    let pw = unsafe { libc::getpwnam(c.as_ptr()) };
    if pw.is_null() {
        return None;
    }
    let dir = unsafe { std::ffi::CStr::from_ptr((*pw).pw_dir) };
    dir.to_str().ok().map(PathBuf::from)
}

pub fn basename(path: &str) -> &str {
    if path.is_empty() {
        return "";
    }
    let trimmed = path.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        return "";
    }
    trimmed.rsplit(['/', '\\']).next().unwrap_or(trimmed)
}

pub fn path_split(path: &str) -> (String, String) {
    if path.is_empty() {
        return (String::new(), String::new());
    }
    let bytes = path.as_bytes();
    let mut index = None;
    for (i, &b) in bytes.iter().enumerate().rev() {
        if b == b'/' || b == b'\\' {
            index = Some(i);
            break;
        }
    }
    match index {
        None => (String::new(), path.to_string()),
        Some(i) => {
            let dir = if i == 0 { path[..1].to_string() } else { path[..i].to_string() };
            (dir, path[i + 1..].to_string())
        }
    }
}

pub fn join_path(base: &str, child: &str) -> String {
    if child.is_empty() {
        return base.to_string();
    }
    if is_absolute(child) {
        return child.to_string();
    }
    if base.is_empty() {
        return child.to_string();
    }
    let sep = if cfg!(windows) { '\\' } else { '/' };
    if base.ends_with(sep) {
        format!("{base}{child}")
    } else {
        format!("{base}{sep}{child}")
    }
}

pub fn is_absolute(path: &str) -> bool {
    path.starts_with('/') || (cfg!(windows) && path.len() >= 3 && path.as_bytes()[1] == b':')
}

pub fn normpath(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let absolute = is_absolute(path);
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if let Some(prev) = parts.last() {
                if *prev != ".." {
                    parts.pop();
                    continue;
                }
            }
            if !absolute {
                parts.push("..");
            }
            continue;
        }
        parts.push(part);
    }
    let sep = if cfg!(windows) { "\\" } else { "/" };
    if absolute {
        format!("{sep}{}", parts.join(sep))
    } else if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join(sep)
    }
}

pub fn getcwd() -> String {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string())
}

pub fn abspath(path: &str) -> String {
    let expanded = if path.starts_with('~') {
        expanduser_path(path).to_string_lossy().to_string()
    } else {
        path.to_string()
    };
    if is_absolute(&expanded) {
        normpath(&expanded)
    } else {
        normpath(&join_path(&getcwd(), &expanded))
    }
}

pub fn is_file(path: &str) -> bool {
    Path::new(path).is_file()
}

pub fn is_dir(path: &str) -> bool {
    Path::new(path).is_dir()
}

pub fn exists(path: &str) -> bool {
    Path::new(path).exists()
}

pub fn listdir(path: &str) -> Vec<String> {
    fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect()
}

pub fn path_sep() -> &'static str {
    if cfg!(windows) { "\\" } else { "/" }
}

/// Python `str.format` for the `{name}` placeholders used by fix_file.
pub fn format_named(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::new();
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '{' {
            if i + 1 < chars.len() && chars[i + 1] == '{' {
                out.push('{');
                i += 2;
                continue;
            }
            let start = i + 1;
            let mut end = start;
            while end < chars.len() && chars[end] != '}' {
                end += 1;
            }
            if end >= chars.len() {
                out.push('{');
                i += 1;
                continue;
            }
            let key: String = chars[start..end].iter().collect();
            if let Some((_, value)) = values.iter().find(|(name, _)| *name == key) {
                out.push_str(value);
            } else {
                out.push('{');
                out.push_str(&key);
                out.push('}');
            }
            i = end + 1;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

pub fn capture(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        None
    }
}

pub fn sudo_support_match(command: &Command, f: impl FnOnce(&Command) -> bool) -> bool {
    if let Some(rest) = command.script.strip_prefix("sudo ") {
        f(&command.with_script(rest))
    } else {
        f(command)
    }
}

pub fn sudo_support_commands(command: &Command, f: impl FnOnce(&Command) -> Vec<String>) -> Vec<String> {
    if let Some(rest) = command.script.strip_prefix("sudo ") {
        let result = f(&command.with_script(rest));
        if result.len() == 1 && result[0].is_empty() {
            result
        } else {
            result.into_iter().map(|line| format!("sudo {line}")).collect()
        }
    } else {
        f(command)
    }
}

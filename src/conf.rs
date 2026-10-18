//! Settings from defaults, `settings.py`, the environment, and CLI flags.
//!
//! File loading accepts the literal subset documented for `settings.py`
//! (booleans, numbers, strings, lists, dicts). A file that is not that subset
//! is ignored, same as a Python exception while importing it.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use crate::const::{self, SETTINGS_DEFAULTS_COMMENTED, SETTINGS_HEADER};
use crate::logs;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuleSelector {
    AllEnabled,
    Named(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum SettingValue {
    None,
    Bool(bool),
    Int(i64),
    Str(String),
    List(Vec<SettingValue>),
    Map(Vec<(String, SettingValue)>),
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub rules: Vec<RuleSelector>,
    pub exclude_rules: Vec<String>,
    pub wait_command: u64,
    pub require_confirmation: bool,
    pub no_colors: bool,
    pub debug: bool,
    pub priority: HashMap<String, i64>,
    pub history_limit: Option<i64>,
    pub alter_history: bool,
    pub wait_slow_command: u64,
    pub slow_commands: Vec<String>,
    pub repeat: bool,
    pub instant_mode: bool,
    pub num_close_matches: usize,
    pub env: HashMap<String, String>,
    pub excluded_search_path_prefixes: Vec<String>,
    pub user_dir: PathBuf,
    pub extras: HashMap<String, SettingValue>,
}

impl Default for Settings {
    fn default() -> Self {
        let mut env = HashMap::new();
        env.insert("LC_ALL".into(), "C".into());
        env.insert("LANG".into(), "C".into());
        env.insert("GIT_TRACE".into(), "1".into());
        Self {
            rules: vec![RuleSelector::AllEnabled],
            exclude_rules: Vec::new(),
            wait_command: 3,
            require_confirmation: true,
            no_colors: false,
            debug: false,
            priority: HashMap::new(),
            history_limit: None,
            alter_history: true,
            wait_slow_command: 15,
            slow_commands: const::SLOW_COMMANDS.iter().map(|s| (*s).to_string()).collect(),
            repeat: false,
            instant_mode: false,
            num_close_matches: 3,
            env,
            excluded_search_path_prefixes: Vec::new(),
            user_dir: PathBuf::new(),
            extras: HashMap::new(),
        }
    }
}

fn slot() -> &'static Mutex<Settings> {
    static SETTINGS: OnceLock<Mutex<Settings>> = OnceLock::new();
    SETTINGS.get_or_init(|| Mutex::new(Settings::default()))
}

fn lock() -> MutexGuard<'static, Settings> {
    slot().lock().unwrap_or_else(|err| err.into_inner())
}

pub fn with<T>(f: impl FnOnce(&Settings) -> T) -> T {
    f(&lock())
}

pub fn with_mut<T>(f: impl FnOnce(&mut Settings) -> T) -> T {
    f(&mut lock())
}

pub fn reset_for_tests() {
    *lock() = Settings::default();
}

pub fn no_colors() -> bool {
    with(|s| s.no_colors)
}

pub fn debug_enabled() -> bool {
    with(|s| s.debug)
}

pub fn require_confirmation() -> bool {
    with(|s| s.require_confirmation)
}

pub fn alter_history() -> bool {
    with(|s| s.alter_history)
}

pub fn repeat_enabled() -> bool {
    with(|s| s.repeat)
}

pub fn instant_mode() -> bool {
    with(|s| s.instant_mode)
}

pub fn num_close_matches() -> usize {
    with(|s| s.num_close_matches)
}

pub fn wait_command() -> u64 {
    with(|s| s.wait_command)
}

pub fn wait_slow_command() -> u64 {
    with(|s| s.wait_slow_command)
}

pub fn slow_commands() -> Vec<String> {
    with(|s| s.slow_commands.clone())
}

pub fn history_limit() -> Option<i64> {
    with(|s| s.history_limit)
}

pub fn excluded_search_path_prefixes() -> Vec<String> {
    with(|s| s.excluded_search_path_prefixes.clone())
}

pub fn subprocess_env() -> HashMap<String, String> {
    with(|s| s.env.clone())
}

pub fn exclude_rules() -> Vec<String> {
    with(|s| s.exclude_rules.clone())
}

pub fn priority_override(name: &str) -> Option<i64> {
    with(|s| s.priority.get(name).copied())
}

pub fn rules_allow_all() -> bool {
    with(|s| s.rules.iter().any(|r| matches!(r, RuleSelector::AllEnabled)))
}

pub fn rules_contain(name: &str) -> bool {
    with(|s| {
        s.rules.iter().any(|r| matches!(r, RuleSelector::Named(n) if n == name))
    })
}

pub fn user_dir() -> PathBuf {
    with(|s| s.user_dir.clone())
}

pub fn setdefault(key: &str, value: SettingValue) {
    with_mut(|s| {
        s.extras.entry(key.to_string()).or_insert(value);
    });
}

pub fn extra(key: &str) -> Option<SettingValue> {
    with(|s| s.extras.get(key).cloned())
}

pub fn extra_str(key: &str) -> Option<String> {
    match extra(key) {
        Some(SettingValue::Str(s)) => Some(s),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct InitArgs {
    pub yes: bool,
    pub debug: bool,
    pub repeat: bool,
}

pub fn init(args: Option<&InitArgs>) {
    with_mut(|settings| {
        if let Err(err) = setup_user_dir(settings) {
            logs::exception("Can't prepare settings directory", &format!("{err}\n"));
        }
    });
    match settings_from_file() {
        Ok(parsed) => apply_file(parsed),
        Err(err) => logs::exception("Can't load settings from file", &format!("{err}\n")),
    }
    match settings_from_env() {
        Ok(parsed) => apply_env(parsed),
        Err(err) => logs::exception("Can't load settings from env", &format!("{err}\n")),
    }
    if let Some(args) = args {
        apply_args(args);
    }
}

fn setup_user_dir(settings: &mut Settings) -> std::io::Result<()> {
    let user_dir = user_dir_path();
    let rules_dir = user_dir.join("rules");
    if !rules_dir.is_dir() {
        fs::create_dir_all(&rules_dir)?;
    }
    settings.user_dir = user_dir.clone();
    let settings_path = user_dir.join("settings.py");
    if !settings_path.is_file() {
        let mut body = String::from(SETTINGS_HEADER);
        body.push_str(SETTINGS_DEFAULTS_COMMENTED);
        fs::write(settings_path, body)?;
    }
    Ok(())
}

fn user_dir_path() -> PathBuf {
    let xdg = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| "~/.config".to_string());
    let user_dir = expanduser(&join_path(&xdg, "revise"));
    let legacy = expanduser("~/.revise");
    if legacy.is_dir() {
        logs::warn(&format!(
            "Config path {} is deprecated. Please move to {}",
            legacy.display(),
            user_dir.display()
        ));
        legacy
    } else {
        user_dir
    }
}

fn expanduser(path: &str) -> PathBuf {
    crate::utils::expanduser_path(path)
}

fn join_path(base: &str, child: &str) -> String {
    if base.is_empty() {
        return child.to_string();
    }
    let sep = std::path::MAIN_SEPARATOR;
    if base.ends_with(sep) {
        format!("{base}{child}")
    } else {
        format!("{base}{sep}{child}")
    }
}

struct FileSettings {
    rules: Option<Vec<RuleSelector>>,
    exclude_rules: Option<Vec<String>>,
    wait_command: Option<u64>,
    require_confirmation: Option<bool>,
    no_colors: Option<bool>,
    debug: Option<bool>,
    priority: Option<HashMap<String, i64>>,
    history_limit: Option<Option<i64>>,
    alter_history: Option<bool>,
    wait_slow_command: Option<u64>,
    slow_commands: Option<Vec<String>>,
    repeat: Option<bool>,
    instant_mode: Option<bool>,
    num_close_matches: Option<usize>,
    env: Option<HashMap<String, String>>,
    excluded_search_path_prefixes: Option<Vec<String>>,
}

fn settings_from_file() -> Result<FileSettings, String> {
    let path = with(|s| s.user_dir.join("settings.py"));
    if path.as_os_str().is_empty() || !path.is_file() {
        return Ok(empty_file());
    }
    let text = fs::read_to_string(&path).map_err(|err| err.to_string())?;
    parse_settings_py(&text)
}

fn empty_file() -> FileSettings {
    FileSettings {
        rules: None,
        exclude_rules: None,
        wait_command: None,
        require_confirmation: None,
        no_colors: None,
        debug: None,
        priority: None,
        history_limit: None,
        alter_history: None,
        wait_slow_command: None,
        slow_commands: None,
        repeat: None,
        instant_mode: None,
        num_close_matches: None,
        env: None,
        excluded_search_path_prefixes: None,
    }
}

fn apply_file(file: FileSettings) {
    with_mut(|s| {
        if let Some(v) = file.rules {
            s.rules = v;
        }
        if let Some(v) = file.exclude_rules {
            s.exclude_rules = v;
        }
        if let Some(v) = file.wait_command {
            s.wait_command = v;
        }
        if let Some(v) = file.require_confirmation {
            s.require_confirmation = v;
        }
        if let Some(v) = file.no_colors {
            s.no_colors = v;
        }
        if let Some(v) = file.debug {
            s.debug = v;
        }
        if let Some(v) = file.priority {
            s.priority = v;
        }
        if let Some(v) = file.history_limit {
            s.history_limit = v;
        }
        if let Some(v) = file.alter_history {
            s.alter_history = v;
        }
        if let Some(v) = file.wait_slow_command {
            s.wait_slow_command = v;
        }
        if let Some(v) = file.slow_commands {
            s.slow_commands = v;
        }
        if let Some(v) = file.repeat {
            s.repeat = v;
        }
        if let Some(v) = file.instant_mode {
            s.instant_mode = v;
        }
        if let Some(v) = file.num_close_matches {
            s.num_close_matches = v;
        }
        if let Some(v) = file.env {
            s.env = v;
        }
        if let Some(v) = file.excluded_search_path_prefixes {
            s.excluded_search_path_prefixes = v;
        }
    });
}

struct EnvSettings {
    rules: Option<Vec<RuleSelector>>,
    exclude_rules: Option<Vec<String>>,
    wait_command: Option<u64>,
    require_confirmation: Option<bool>,
    no_colors: Option<bool>,
    debug: Option<bool>,
    priority: Option<HashMap<String, i64>>,
    history_limit: Option<i64>,
    alter_history: Option<bool>,
    wait_slow_command: Option<u64>,
    slow_commands: Option<Vec<String>>,
    repeat: Option<bool>,
    instant_mode: Option<bool>,
    num_close_matches: Option<usize>,
    excluded_search_path_prefixes: Option<Vec<String>>,
}

fn settings_from_env() -> Result<EnvSettings, String> {
    let mut env = EnvSettings {
        rules: None,
        exclude_rules: None,
        wait_command: None,
        require_confirmation: None,
        no_colors: None,
        debug: None,
        priority: None,
        history_limit: None,
        alter_history: None,
        wait_slow_command: None,
        slow_commands: None,
        repeat: None,
        instant_mode: None,
        num_close_matches: None,
        excluded_search_path_prefixes: None,
    };
    if let Ok(val) = std::env::var("REVISE_RULES") {
        env.rules = Some(rules_from_env(&val));
    }
    if let Ok(val) = std::env::var("REVISE_EXCLUDE_RULES") {
        env.exclude_rules = Some(split_colon(&val));
    }
    if let Ok(val) = std::env::var("REVISE_WAIT_COMMAND") {
        env.wait_command = Some(parse_u64(&val)?);
    }
    if let Ok(val) = std::env::var("REVISE_REQUIRE_CONFIRMATION") {
        env.require_confirmation = Some(val.eq_ignore_ascii_case("true"));
    }
    if let Ok(val) = std::env::var("REVISE_NO_COLORS") {
        env.no_colors = Some(val.eq_ignore_ascii_case("true"));
    }
    if let Ok(val) = std::env::var("REVISE_DEBUG") {
        env.debug = Some(val.eq_ignore_ascii_case("true"));
    }
    if let Ok(val) = std::env::var("REVISE_PRIORITY") {
        env.priority = Some(priority_from_env(&val));
    }
    if let Ok(val) = std::env::var("REVISE_HISTORY_LIMIT") {
        env.history_limit = Some(parse_i64(&val)?);
    }
    if let Ok(val) = std::env::var("REVISE_ALTER_HISTORY") {
        env.alter_history = Some(val.eq_ignore_ascii_case("true"));
    }
    if let Ok(val) = std::env::var("REVISE_WAIT_SLOW_COMMAND") {
        env.wait_slow_command = Some(parse_u64(&val)?);
    }
    if let Ok(val) = std::env::var("REVISE_SLOW_COMMANDS") {
        env.slow_commands = Some(split_colon(&val));
    }
    if let Ok(val) = std::env::var("REVISE_REPEAT") {
        // The Python loader returns the raw string. Any non-empty string is truthy,
        // including the text "false".
        env.repeat = Some(!val.is_empty());
    }
    if let Ok(val) = std::env::var("REVISE_INSTANT_MODE") {
        env.instant_mode = Some(val.eq_ignore_ascii_case("true"));
    }
    if let Ok(val) = std::env::var("REVISE_NUM_CLOSE_MATCHES") {
        let n = parse_i64(&val)?;
        if n < 0 {
            return Err(format!("num_close_matches must be >= 0, got {n}"));
        }
        env.num_close_matches = Some(n as usize);
    }
    if let Ok(val) = std::env::var("REVISE_EXCLUDED_SEARCH_PATH_PREFIXES") {
        env.excluded_search_path_prefixes = Some(split_colon(&val));
    }
    Ok(env)
}

fn apply_env(env: EnvSettings) {
    with_mut(|s| {
        if let Some(v) = env.rules {
            s.rules = v;
        }
        if let Some(v) = env.exclude_rules {
            s.exclude_rules = v;
        }
        if let Some(v) = env.wait_command {
            s.wait_command = v;
        }
        if let Some(v) = env.require_confirmation {
            s.require_confirmation = v;
        }
        if let Some(v) = env.no_colors {
            s.no_colors = v;
        }
        if let Some(v) = env.debug {
            s.debug = v;
        }
        if let Some(v) = env.priority {
            s.priority = v;
        }
        if let Some(v) = env.history_limit {
            s.history_limit = Some(v);
        }
        if let Some(v) = env.alter_history {
            s.alter_history = v;
        }
        if let Some(v) = env.wait_slow_command {
            s.wait_slow_command = v;
        }
        if let Some(v) = env.slow_commands {
            s.slow_commands = v;
        }
        if let Some(v) = env.repeat {
            s.repeat = v;
        }
        if let Some(v) = env.instant_mode {
            s.instant_mode = v;
        }
        if let Some(v) = env.num_close_matches {
            s.num_close_matches = v;
        }
        if let Some(v) = env.excluded_search_path_prefixes {
            s.excluded_search_path_prefixes = v;
        }
    });
}

fn apply_args(args: &InitArgs) {
    with_mut(|s| {
        if args.yes {
            s.require_confirmation = false;
        }
        if args.debug {
            s.debug = true;
        }
        if args.repeat {
            s.repeat = true;
        }
    });
}

fn rules_from_env(val: &str) -> Vec<RuleSelector> {
    let parts: Vec<&str> = val.split(':').collect();
    if parts.iter().any(|part| *part == "DEFAULT_RULES") {
        let mut rules = vec![RuleSelector::AllEnabled];
        for part in parts {
            if part != "DEFAULT_RULES" {
                rules.push(RuleSelector::Named(part.to_string()));
            }
        }
        rules
    } else {
        parts
            .into_iter()
            .map(|part| RuleSelector::Named(part.to_string()))
            .collect()
    }
}

fn priority_from_env(val: &str) -> HashMap<String, i64> {
    let mut map = HashMap::new();
    for part in val.split(':') {
        let mut pieces = part.split('=');
        let Some(rule) = pieces.next() else { continue };
        let Some(priority) = pieces.next() else { continue };
        if pieces.next().is_some() || rule.is_empty() {
            continue;
        }
        if let Ok(priority) = priority.parse::<i64>() {
            map.insert(rule.to_string(), priority);
        }
    }
    map
}

fn split_colon(val: &str) -> Vec<String> {
    val.split(':').map(|part| part.to_string()).collect()
}

fn parse_i64(val: &str) -> Result<i64, String> {
    val.parse::<i64>().map_err(|err| err.to_string())
}

fn parse_u64(val: &str) -> Result<u64, String> {
    val.parse::<u64>().map_err(|err| err.to_string())
}

fn parse_settings_py(text: &str) -> Result<FileSettings, String> {
    let mut file = empty_file();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        skip_ws_and_comments(&chars, &mut i);
        if i >= chars.len() {
            break;
        }
        let name = parse_ident(&chars, &mut i).ok_or_else(|| "expected a setting name".to_string())?;
        skip_ws_and_comments(&chars, &mut i);
        if i >= chars.len() || chars[i] != '=' {
            return Err(format!("expected '=' after {name}"));
        }
        i += 1;
        skip_ws_and_comments(&chars, &mut i);
        let value = parse_value(&chars, &mut i)?;
        assign(&mut file, &name, value)?;
        skip_ws_and_comments(&chars, &mut i);
        if i < chars.len() && chars[i] == ';' {
            i += 1;
        }
    }
    Ok(file)
}

fn assign(file: &mut FileSettings, name: &str, value: SettingValue) -> Result<(), String> {
    match name {
        "rules" => file.rules = Some(value_rules(value)?),
        "exclude_rules" => file.exclude_rules = Some(value_strings(value)?),
        "wait_command" => file.wait_command = Some(value_u64(value)?),
        "require_confirmation" => file.require_confirmation = Some(value_bool(value)?),
        "no_colors" => file.no_colors = Some(value_bool(value)?),
        "debug" => file.debug = Some(value_bool(value)?),
        "priority" => file.priority = Some(value_priority(value)?),
        "history_limit" => file.history_limit = Some(value_optional_i64(value)?),
        "alter_history" => file.alter_history = Some(value_bool(value)?),
        "wait_slow_command" => file.wait_slow_command = Some(value_u64(value)?),
        "slow_commands" => file.slow_commands = Some(value_strings(value)?),
        "repeat" => file.repeat = Some(value_bool(value)?),
        "instant_mode" => file.instant_mode = Some(value_bool(value)?),
        "num_close_matches" => file.num_close_matches = Some(value_usize(value)?),
        "env" => file.env = Some(value_string_map(value)?),
        "excluded_search_path_prefixes" => {
            file.excluded_search_path_prefixes = Some(value_strings(value)?)
        }
        _ => {}
    }
    Ok(())
}

fn value_bool(value: SettingValue) -> Result<bool, String> {
    match value {
        SettingValue::Bool(v) => Ok(v),
        _ => Err("expected a boolean".into()),
    }
}

fn value_u64(value: SettingValue) -> Result<u64, String> {
    match value {
        SettingValue::Int(v) if v >= 0 => Ok(v as u64),
        _ => Err("expected a non-negative integer".into()),
    }
}

fn value_usize(value: SettingValue) -> Result<usize, String> {
    Ok(value_u64(value)? as usize)
}

fn value_optional_i64(value: SettingValue) -> Result<Option<i64>, String> {
    match value {
        SettingValue::None => Ok(None),
        SettingValue::Int(v) => Ok(Some(v)),
        _ => Err("expected an integer or None".into()),
    }
}

fn value_strings(value: SettingValue) -> Result<Vec<String>, String> {
    match value {
        SettingValue::List(items) => items
            .into_iter()
            .map(|item| match item {
                SettingValue::Str(s) => Ok(s),
                _ => Err("expected a list of strings".into()),
            })
            .collect(),
        _ => Err("expected a list".into()),
    }
}

fn value_rules(value: SettingValue) -> Result<Vec<RuleSelector>, String> {
    Ok(value_strings(value)?
        .into_iter()
        .map(RuleSelector::Named)
        .collect())
}

fn value_priority(value: SettingValue) -> Result<HashMap<String, i64>, String> {
    match value {
        SettingValue::Map(entries) => {
            let mut map = HashMap::new();
            for (key, val) in entries {
                match val {
                    SettingValue::Int(n) => {
                        map.insert(key, n);
                    }
                    _ => return Err("priority values must be integers".into()),
                }
            }
            Ok(map)
        }
        _ => Err("expected a dict".into()),
    }
}

fn value_string_map(value: SettingValue) -> Result<HashMap<String, String>, String> {
    match value {
        SettingValue::Map(entries) => {
            let mut map = HashMap::new();
            for (key, val) in entries {
                match val {
                    SettingValue::Str(s) => {
                        map.insert(key, s);
                    }
                    _ => return Err("env values must be strings".into()),
                }
            }
            Ok(map)
        }
        _ => Err("expected a dict".into()),
    }
}

fn skip_ws_and_comments(chars: &[char], i: &mut usize) {
    loop {
        while *i < chars.len() && chars[*i].is_whitespace() {
            *i += 1;
        }
        if *i < chars.len() && chars[*i] == '#' {
            while *i < chars.len() && chars[*i] != '\n' {
                *i += 1;
            }
            continue;
        }
        break;
    }
}

fn parse_ident(chars: &[char], i: &mut usize) -> Option<String> {
    if *i >= chars.len() || !(chars[*i].is_ascii_alphabetic() || chars[*i] == '_') {
        return None;
    }
    let start = *i;
    *i += 1;
    while *i < chars.len() && (chars[*i].is_ascii_alphanumeric() || chars[*i] == '_') {
        *i += 1;
    }
    Some(chars[start..*i].iter().collect())
}

fn parse_value(chars: &[char], i: &mut usize) -> Result<SettingValue, String> {
    if *i >= chars.len() {
        return Err("expected a value".into());
    }
    match chars[*i] {
        '[' => parse_list(chars, i),
        '{' => parse_dict(chars, i),
        '\'' | '"' => Ok(SettingValue::Str(parse_string(chars, i)?)),
        _ => {
            if let Some(name) = parse_ident(chars, i) {
                return match name.as_str() {
                    "None" => Ok(SettingValue::None),
                    "True" => Ok(SettingValue::Bool(true)),
                    "False" => Ok(SettingValue::Bool(false)),
                    other => Err(format!("unknown name {other}")),
                };
            }
            parse_number(chars, i)
        }
    }
}

fn parse_number(chars: &[char], i: &mut usize) -> Result<SettingValue, String> {
    let start = *i;
    if *i < chars.len() && (chars[*i] == '-' || chars[*i] == '+') {
        *i += 1;
    }
    let digits_at = *i;
    while *i < chars.len() && chars[*i].is_ascii_digit() {
        *i += 1;
    }
    if *i == digits_at {
        return Err("expected a value".into());
    }
    let text: String = chars[start..*i].iter().collect();
    text.parse::<i64>()
        .map(SettingValue::Int)
        .map_err(|err| err.to_string())
}

fn parse_list(chars: &[char], i: &mut usize) -> Result<SettingValue, String> {
    *i += 1;
    let mut items = Vec::new();
    loop {
        skip_ws_and_comments(chars, i);
        if *i >= chars.len() {
            return Err("unterminated list".into());
        }
        if chars[*i] == ']' {
            *i += 1;
            break;
        }
        items.push(parse_value(chars, i)?);
        skip_ws_and_comments(chars, i);
        if *i < chars.len() && chars[*i] == ',' {
            *i += 1;
            continue;
        }
        skip_ws_and_comments(chars, i);
        if *i < chars.len() && chars[*i] == ']' {
            *i += 1;
            break;
        }
        return Err("expected ',' or ']'".into());
    }
    Ok(SettingValue::List(items))
}

fn parse_dict(chars: &[char], i: &mut usize) -> Result<SettingValue, String> {
    *i += 1;
    let mut entries = Vec::new();
    loop {
        skip_ws_and_comments(chars, i);
        if *i >= chars.len() {
            return Err("unterminated dict".into());
        }
        if chars[*i] == '}' {
            *i += 1;
            break;
        }
        let key = if chars[*i] == '\'' || chars[*i] == '"' {
            parse_string(chars, i)?
        } else if let Some(name) = parse_ident(chars, i) {
            name
        } else {
            return Err("expected a dict key".into());
        };
        skip_ws_and_comments(chars, i);
        if *i >= chars.len() || chars[*i] != ':' {
            return Err("expected ':'".into());
        }
        *i += 1;
        skip_ws_and_comments(chars, i);
        let value = parse_value(chars, i)?;
        entries.push((key, value));
        skip_ws_and_comments(chars, i);
        if *i < chars.len() && chars[*i] == ',' {
            *i += 1;
            continue;
        }
        skip_ws_and_comments(chars, i);
        if *i < chars.len() && chars[*i] == '}' {
            *i += 1;
            break;
        }
        return Err("expected ',' or '}'".into());
    }
    Ok(SettingValue::Map(entries))
}

fn parse_string(chars: &[char], i: &mut usize) -> Result<String, String> {
    let quote = chars[*i];
    *i += 1;
    let triple = *i + 1 < chars.len() && chars[*i] == quote && chars[*i + 1] == quote;
    if triple {
        *i += 2;
    }
    let mut out = String::new();
    while *i < chars.len() {
        let ch = chars[*i];
        *i += 1;
        if triple {
            if ch == quote && *i + 1 < chars.len() && chars[*i] == quote && chars[*i + 1] == quote {
                *i += 2;
                return Ok(out);
            }
        } else if ch == quote {
            return Ok(out);
        }
        if ch == '\\' {
            if *i >= chars.len() {
                return Err("truncated string escape".into());
            }
            let esc = chars[*i];
            *i += 1;
            out.push(match esc {
                '\\' => '\\',
                '\'' => '\'',
                '"' => '"',
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                other => {
                    out.push('\\');
                    other
                }
            });
        } else {
            out.push(ch);
        }
    }
    Err("unterminated string".into())
}

pub fn debug_dump() -> String {
    with(|s| format!("{s:?}"))
}

pub fn user_rules_dir() -> PathBuf {
    with(|s| s.user_dir.join("rules"))
}

pub fn legacy_note_path(path: &Path) -> String {
    path.display().to_string()
}

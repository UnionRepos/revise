//! stderr / stdout messages. Formats match `revise.logs`.

use std::io::{self, Write};
use std::time::Instant;

use crate::conf;
use crate::const::USER_COMMAND_MARK;

const RESET: &str = "\u{1b}[0m";
const BOLD: &str = "\u{1b}[1m";
const RED: &str = "\u{1b}[31m";
const GREEN: &str = "\u{1b}[32m";
const BLUE: &str = "\u{1b}[34m";
const WHITE: &str = "\u{1b}[37m";
const BG_RED: &str = "\u{1b}[41m";

fn paint(code: &str) -> &str {
    if conf::no_colors() {
        ""
    } else {
        code
    }
}

fn warn_style() -> String {
    format!("{}{}{}", paint(BG_RED), paint(WHITE), paint(BOLD))
}

fn stderr(text: &str) {
    let mut err = io::stderr();
    let _ = err.write_all(text.as_bytes());
    let _ = err.flush();
}

fn stdout(text: &str) {
    let mut out = io::stdout();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

pub fn warn(title: &str) {
    stderr(&format!("{}[WARN] {}{}\n", warn_style(), title, paint(RESET)));
}

pub fn exception(title: &str, trace: &str) {
    stderr(&format!(
        "{}[WARN] {}: {}\n{}{}----------------------------{}\n\n",
        warn_style(),
        title,
        paint(RESET),
        trace,
        warn_style(),
        paint(RESET)
    ));
}

pub fn rule_failed(name: &str) {
    exception(&format!("Rule {name}"), "");
}

pub fn failed(msg: &str) {
    stderr(&format!("{}{msg}{}\n", paint(RED), paint(RESET)));
}

pub fn show_corrected_command(script: &str, side_effect: bool) {
    let side = if side_effect { " (+side effect)" } else { "" };
    stderr(&format!(
        "{USER_COMMAND_MARK}{}{script}{}{side}\n",
        paint(BOLD),
        paint(RESET)
    ));
}

pub fn confirm_text(script: &str, side_effect: bool) {
    let side = if side_effect { " (+side effect)" } else { "" };
    stderr(&format!(
        "{USER_COMMAND_MARK}\u{1b}[1K\r{}{script}{}{side} [{green}enter{reset}/{blue}↑{reset}/{blue}↓{reset}/{red}ctrl+c{reset}]",
        paint(BOLD),
        paint(RESET),
        green = paint(GREEN),
        reset = paint(RESET),
        blue = paint(BLUE),
        red = paint(RED),
    ));
}

pub fn debug(msg: &str) {
    if conf::debug_enabled() {
        stderr(&format!(
            "{}{}DEBUG:{} {msg}\n",
            paint(BLUE),
            paint(BOLD),
            paint(RESET)
        ));
    }
}

pub struct DebugTimer {
    msg: String,
    started: Instant,
}

impl DebugTimer {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            msg: msg.into(),
            started: Instant::now(),
        }
    }
}

impl Drop for DebugTimer {
    fn drop(&mut self) {
        debug(&format!("{} took: {:?}", self.msg, self.started.elapsed()));
    }
}

pub fn how_to_configure_alias(details: Option<&crate::shells::ShellConfiguration>) {
    stdout(&format!(
        "Seems like {}revise{} alias isn't configured!\n",
        paint(BOLD),
        paint(RESET)
    ));
    if let Some(details) = details {
        stdout(&format!(
            "Please put {bold}{content}{reset} in your {bold}{path}{reset} and apply changes with {bold}{reload}{reset} or restart your shell.\n",
            bold = paint(BOLD),
            reset = paint(RESET),
            content = details.content,
            path = details.path,
            reload = details.reload,
        ));
        if details.can_configure_automatically {
            stdout(&format!(
                "Or run {bold}revise{reset} a second time to configure it automatically.\n",
                bold = paint(BOLD),
                reset = paint(RESET)
            ));
        }
    }
    stdout("More details - https://github.com/nvbn/revise#manual-installation\n");
}

pub fn already_configured(reload: &str) {
    stdout(&format!(
        "Seems like {bold}revise{reset} alias already configured!\nFor applying changes run {bold}{reload}{reset} or restart your shell.\n",
        bold = paint(BOLD),
        reset = paint(RESET),
    ));
}

pub fn configured_successfully(reload: &str) {
    stdout(&format!(
        "{bold}revise{reset} alias configured successfully!\nFor applying changes run {bold}{reload}{reset} or restart your shell.\n",
        bold = paint(BOLD),
        reset = paint(RESET),
    ));
}

pub fn version(shell_info: &str) {
    stderr(&format!(
        "Revise {} using Rust {} and {}\n",
        crate::const::VERSION,
        env!("REVISE_RUSTC_VERSION"),
        shell_info
    ));
}

//! OS helpers: raw keys, process tree, terminal size, user name.

use std::io::{self, Read};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    CtrlC,
    CtrlN,
    CtrlP,
    Char(char),
    Other(u8),
}

pub fn init_output() {
    // colorama.init() is a no-op on Unix. Windows consoles already get ANSI
    // from the terminal host on current Windows; nothing to wrap.
}

pub fn open_command(arg: &str) -> String {
    if crate::utils::which("xdg-open").is_some() {
        format!("xdg-open {arg}")
    } else if cfg!(windows) {
        format!("cmd /c start {arg}")
    } else {
        format!("open {arg}")
    }
}

pub fn username() -> String {
    for key in ["LOGNAME", "USER", "LNAME", "USERNAME"] {
        if let Ok(value) = std::env::var(key) {
            if !value.is_empty() {
                return value;
            }
        }
    }
    let uid = unsafe { libc::getuid() };
    let pw = unsafe { libc::getpwuid(uid) };
    if !pw.is_null() {
        let name = unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) };
        if let Ok(text) = name.to_str() {
            return text.to_string();
        }
    }
    "unknown".to_string()
}

pub fn temp_dir() -> PathBuf {
    std::env::temp_dir()
}

pub fn terminal_columns() -> u16 {
    unsafe {
        let mut size: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut size) == 0 && size.ws_col > 0 {
            return size.ws_col;
        }
        if libc::ioctl(libc::STDIN_FILENO, libc::TIOCGWINSZ, &mut size) == 0 && size.ws_col > 0 {
            return size.ws_col;
        }
    }
    if let Ok(cols) = std::env::var("COLUMNS") {
        if let Ok(n) = cols.parse::<u16>() {
            if n > 0 {
                return n;
            }
        }
    }
    80
}

pub fn current_pid() -> u32 {
    std::process::id()
}

pub fn parent_pid(pid: u32) -> Option<u32> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("PPid:") {
            return rest.trim().parse().ok();
        }
    }
    None
}

pub fn process_name(pid: u32) -> Option<String> {
    let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    let name = comm.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Descendants, deepest first, so they can be signalled before their parents.
pub fn descendants(pid: u32) -> Vec<u32> {
    let mut children: Vec<(u32, u32)> = Vec::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let Ok(child) = name.parse::<u32>() else {
            continue;
        };
        if let Some(parent) = parent_pid(child) {
            children.push((parent, child));
        }
    }
    let mut out = Vec::new();
    fn walk(pid: u32, edges: &[(u32, u32)], out: &mut Vec<u32>) {
        for &(parent, child) in edges {
            if parent == pid {
                walk(child, edges, out);
                out.push(child);
            }
        }
    }
    walk(pid, &children, &mut out);
    out
}

pub fn kill_pid(pid: u32) -> Result<(), ()> {
    let rc = unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    if rc == 0 {
        Ok(())
    } else {
        Err(())
    }
}

fn read_byte() -> io::Result<u8> {
    let mut buf = [0u8; 1];
    let n = io::stdin().read(&mut buf)?;
    if n == 0 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "stdin closed"));
    }
    Ok(buf[0])
}

/// Read one key in raw mode. Errors when stdin is not a terminal.
pub fn get_key() -> io::Result<Key> {
    unsafe {
        let fd = libc::STDIN_FILENO;
        let mut old: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut old) != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut raw = old;
        libc::cfmakeraw(&mut raw);
        if libc::tcsetattr(fd, libc::TCSAFLUSH, &raw) != 0 {
            return Err(io::Error::last_os_error());
        }
        let result = read_key_raw();
        libc::tcsetattr(fd, libc::TCSADRAIN, &old);
        result
    }
}

fn read_key_raw() -> io::Result<Key> {
    let byte = read_byte()?;
    if byte == crate::const::KEY_MAPPING_CTRL_N {
        return Ok(Key::CtrlN);
    }
    if byte == crate::const::KEY_MAPPING_CTRL_C {
        return Ok(Key::CtrlC);
    }
    if byte == crate::const::KEY_MAPPING_CTRL_P {
        return Ok(Key::CtrlP);
    }
    if byte == 0x1b {
        // Python returns the original ESC unless the sequence is an arrow.
        // The following bytes are consumed either way.
        let second = read_byte()?;
        if second == b'[' {
            let third = read_byte()?;
            return match third {
                b'A' => Ok(Key::Up),
                b'B' => Ok(Key::Down),
                _ => Ok(Key::Other(0x1b)),
            };
        }
        return Ok(Key::Other(0x1b));
    }
    if byte.is_ascii() && !byte.is_ascii_control() {
        Ok(Key::Char(byte as char))
    } else {
        Ok(Key::Other(byte))
    }
}

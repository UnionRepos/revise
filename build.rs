fn main() {
    let version = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            let text = String::from_utf8_lossy(&out.stdout).to_string();
            text.split_whitespace().nth(1).map(|part| part.to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=REVISE_RUSTC_VERSION={version}");
}

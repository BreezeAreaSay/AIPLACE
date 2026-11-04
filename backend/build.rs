//! Copies the built frontend (`../frontend/dist`) into OUT_DIR so `rust-embed` can bake it
//! into the binary. Without a build a placeholder page is embedded instead, so `cargo run`
//! always works (API + hint page) and `npm run dev` can proxy to it.

use std::{env, fs, path::Path};

const PLACEHOLDER: &str = r#"<!doctype html><html lang="ru"><head><meta charset="utf-8"><title>Triad Canvas</title>
<style>body{background:#0D0E12;color:#ccc;font:14px/1.6 ui-monospace,monospace;display:grid;place-items:center;height:100vh;margin:0}
code{color:#CCFF00}div{max-width:520px;padding:24px;border:1px solid #23242e;border-radius:8px}</style></head>
<body><div><b style="color:#CCFF00">TRIAD CANVAS</b> · API работает, но фронтенд в этот бинарник не встроен.<br><br>
Dev: <code>cd frontend && npm run dev</code> → http://localhost:5173<br>
Release: <code>cd frontend && npm run build</code>, затем <code>cargo build --release</code> — dist встроится автоматически.</div></body></html>"#;

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

fn main() {
    let out = Path::new(&env::var("OUT_DIR").expect("OUT_DIR")).join("web");
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&out).expect("create OUT_DIR/web");
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../frontend/dist");
    println!("cargo:rerun-if-changed=../frontend/dist");
    println!("cargo:rerun-if-changed=../frontend/dist/index.html");
    println!("cargo:rerun-if-env-changed=TRIAD_SKIP_EMBED");
    let embed = dist.join("index.html").exists() && env::var("TRIAD_SKIP_EMBED").is_err();
    if embed {
        copy_dir(&dist, &out).expect("copy frontend/dist");
    } else {
        fs::write(out.join("index.html"), PLACEHOLDER).expect("write placeholder");
    }
    println!("cargo:rustc-env=TRIAD_WEB_EMBEDDED={}", if embed { "1" } else { "0" });
}

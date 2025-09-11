use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use clap::Subcommand;
use sha2::Digest;
use sha2::Sha256;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use tempfile::TempDir;
use walkdir::WalkDir;

#[derive(Debug, Parser)]
#[clap(author, version, about = "Codex Workflow POC tools")]
struct Cli {
    #[clap(subcommand)]
    cmd: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Render a code template directory with simple {{key}} substitution.
    RenderTemplate {
        /// Template directory to render (source).
        #[arg(long, value_name = "DIR")]
        template_dir: PathBuf,

        /// Target directory to compare/apply (destination root).
        #[arg(long, value_name = "DIR")]
        target_dir: PathBuf,

        /// Apply changes (default: preview only with unified diff).
        #[arg(long, default_value_t = false)]
        apply: bool,

        /// Key=Value params for {{key}} placeholders (repeatable).
        #[arg(long = "param", value_name = "K=V")]
        params: Vec<String>,
    },

    /// Run a local script (sh/js). Minimal POC.
    ScriptRun {
        /// Script path (.sh or .js)
        #[arg(long, value_name = "FILE")]
        entry: PathBuf,

        /// Arguments to pass to the script (after --)
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Pack .codex/{workflows,templates,registry.yml} into a tar + manifest.json
    Pack {
        /// Path to .codex directory (default: ./.codex)
        #[arg(long, default_value = ".codex")]
        source: PathBuf,

        /// Output tar archive path
        #[arg(long, default_value = "codex-workflows.tar")]
        out: PathBuf,
    },

    /// Unpack an archive to .codex/shared and verify integrity via manifest
    Unpack {
        /// Archive path
        #[arg(long)]
        archive: PathBuf,

        /// Destination directory (default: ./.codex/shared)
        #[arg(long, default_value = ".codex/shared")]
        dest: PathBuf,
    },
}

fn parse_params(params: &[String]) -> Result<BTreeMap<String, String>> {
    let mut map = BTreeMap::new();
    for p in params {
        if let Some((k, v)) = p.split_once('=') {
            map.insert(k.to_string(), v.to_string());
        } else {
            bail!("Invalid param: {p} (expected K=V)");
        }
    }
    Ok(map)
}

fn substitute_placeholders(input: &str, params: &BTreeMap<String, String>) -> String {
    // Very naive: replace occurrences of {{ key }} with the provided value.
    // Whitespace around the key is trimmed.
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    let bytes = input.as_bytes();
    while i < bytes.len() {
        if i + 3 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            // find closing }}
            if let Some(end) = input[i + 2..].find("}}").map(|x| x + i + 2) {
                let key_raw = &input[i + 2..end];
                let key = key_raw.trim();
                if let Some(val) = params.get(key) {
                    out.push_str(val);
                } else {
                    // Unknown key: keep as-is
                    out.push_str("{{");
                    out.push_str(key_raw);
                    out.push_str("}}");
                }
                i = end + 2;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn copy_dir_with_render(src: &Path, dst: &Path, params: &BTreeMap<String, String>) -> Result<()> {
    for entry in WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.is_dir() {
            continue;
        }
        let rel = p.strip_prefix(src).unwrap();
        // Render contents
        let content =
            fs::read_to_string(p).with_context(|| format!("read file: {}", p.display()))?;
        let rendered = substitute_placeholders(&content, params);

        let out_path = dst.join(rel);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("mkdirs: {}", parent.display()))?;
        }
        fs::write(&out_path, rendered)
            .with_context(|| format!("write file: {}", out_path.display()))?;
    }
    Ok(())
}

fn run_git_diff_dir(a: &Path, b: &Path) -> Result<()> {
    if which::which("git").is_err() {
        println!(
            "git not found; showing file lists only. A: {} B: {}",
            a.display(),
            b.display()
        );
        return Ok(());
    }
    let output = std::process::Command::new("git")
        .args(["diff", "--no-index", "-r"]) // recursive dir diff
        .arg(a)
        .arg(b)
        .output()
        .context("run git diff")?;
    // git diff returns exit code 1 when differences are found; treat both 0 and 1 as success.
    if output.status.code().unwrap_or(2) > 1 {
        bail!(
            "git diff failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    print!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path, overwrite: bool) -> Result<()> {
    for entry in WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        let rel = p.strip_prefix(src).unwrap();
        let out_path = dst.join(rel);
        if p.is_dir() {
            fs::create_dir_all(&out_path)?;
        } else {
            if out_path.exists() && !overwrite {
                bail!(
                    "Refusing to overwrite existing file: {}",
                    out_path.display()
                );
            }
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(p, &out_path)
                .with_context(|| format!("copy {} -> {}", p.display(), out_path.display()))?;
        }
    }
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ManifestEntry {
    path: String,
    sha256: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Manifest {
    entries: Vec<ManifestEntry>,
    source: String,
    version: String,
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn pack_codex(source: &Path, out: &Path) -> Result<PathBuf> {
    let mut builder = tar::Builder::new(fs::File::create(out)?);
    let mut entries: Vec<ManifestEntry> = vec![];
    for name in ["workflows", "templates", "registry.yml"] {
        let p = source.join(name);
        if !p.exists() {
            continue;
        }
        if p.is_dir() {
            for entry in WalkDir::new(&p).into_iter().filter_map(|e| e.ok()) {
                let ep = entry.path();
                if ep.is_dir() {
                    continue;
                }
                let mut file = fs::File::open(ep)?;
                let rel = ep.strip_prefix(source).unwrap();
                builder.append_file(rel, &mut file)?;
                let hash = sha256_file(ep)?;
                entries.push(ManifestEntry {
                    path: rel.to_string_lossy().into_owned(),
                    sha256: hash,
                });
            }
        } else {
            let mut file = fs::File::open(&p)?;
            builder.append_file(Path::new(name), &mut file)?;
            let hash = sha256_file(&p)?;
            entries.push(ManifestEntry {
                path: name.to_string(),
                sha256: hash,
            });
        }
    }
    builder.finish()?;
    let manifest = Manifest {
        entries,
        source: source.to_string_lossy().into_owned(),
        version: "v1".into(),
    };
    let manifest_path = out.with_extension("tar.manifest.json");
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
    Ok(manifest_path)
}

fn unpack_codex(archive: &Path, dest: &Path) -> Result<()> {
    let manifest_path = archive.with_extension("tar.manifest.json");
    if !manifest_path.exists() {
        println!("warning: manifest not found: {}", manifest_path.display());
    }
    fs::create_dir_all(dest)?;
    let file = fs::File::open(archive)?;
    let mut ar = tar::Archive::new(file);
    ar.unpack(dest)?;
    // verify if manifest present
    if manifest_path.exists() {
        let mf: Manifest = serde_json::from_slice(&fs::read(&manifest_path)?)?;
        let mut mismatches = 0usize;
        for e in mf.entries {
            let p = dest.join(e.path);
            match sha256_file(&p) {
                Ok(h) => {
                    if h != e.sha256 {
                        eprintln!("hash mismatch: {}", p.display());
                        mismatches += 1;
                    }
                }
                Err(err) => {
                    eprintln!("missing file {}: {err}", p.display());
                    mismatches += 1;
                }
            }
        }
        if mismatches > 0 {
            bail!("unpack completed with {mismatches} integrity issue(s)");
        }
    }
    Ok(())
}

fn run_script(entry: &Path, args: &[String]) -> Result<i32> {
    let ext = entry.extension().and_then(OsStr::to_str).unwrap_or("");
    let (prog, mut argv): (&str, Vec<String>) = match ext {
        "sh" => ("sh", vec![entry.to_string_lossy().into_owned()]),
        "js" => ("node", vec![entry.to_string_lossy().into_owned()]),
        "ts" => {
            bail!("TypeScript not supported in POC. Use JS or SH for now.");
        }
        _ => bail!("Unsupported script extension: .{ext} (use .sh or .js)"),
    };
    argv.extend(args.iter().cloned());
    let status = std::process::Command::new(prog)
        .args(&argv)
        .status()
        .with_context(|| format!("spawn {prog} with {argv:?}"))?;
    Ok(status.code().unwrap_or(1))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Command::RenderTemplate {
            template_dir,
            target_dir,
            apply,
            params,
        } => {
            if !template_dir.is_dir() {
                bail!("template_dir must be a directory");
            }
            if !target_dir.is_dir() {
                bail!("target_dir must be a directory");
            }
            let params = parse_params(&params)?;
            let tmp: TempDir = tempfile::tempdir().context("create temp dir")?;
            let rendered = tmp.path().join("rendered");
            fs::create_dir_all(&rendered)?;
            copy_dir_with_render(&template_dir, &rendered, &params)?;
            if apply {
                copy_dir_recursive(&rendered, &target_dir, false)?;
                println!("Applied rendered templates to {}", target_dir.display());
            } else {
                println!("Preview (unified diff). Left: target, Right: rendered");
                run_git_diff_dir(&target_dir, &rendered)?;
                println!("(use --apply to write files)");
            }
        }
        Command::ScriptRun { entry, args } => {
            if !entry.exists() {
                bail!("script not found: {}", entry.display());
            }
            let code = run_script(&entry, &args)?;
            if code != 0 {
                std::process::exit(code);
            }
        }
        Command::Pack { source, out } => {
            let manifest_path = pack_codex(&source, &out)?;
            println!("Wrote {} and {}", out.display(), manifest_path.display());
        }
        Command::Unpack { archive, dest } => {
            unpack_codex(&archive, &dest)?;
            println!("Unpacked {} to {}", archive.display(), dest.display());
        }
    }
    Ok(())
}

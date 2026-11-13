use clap::{Parser, Subcommand};
use magic_hotkey_lib::{commands, load_commands, mask::Masker, run_action, secrets};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// mh — Magic Hotkey CLI
///
/// Run transformations, generate values, and manage secrets from the terminal.
/// All commands read from stdin (unless they generate output) and write to stdout.
///
/// Examples:
///   echo "hello" | mh encode base64
///   echo '{"a":1}' | mh fmt json
///   mh gen guid
///   cat config.yaml | mh convert yaml json
///   echo "secret" | mh pipe base64_encode uppercase
///   mh run "My Pipeline" < input.txt
#[derive(Parser)]
#[command(name = "mh", version, about, long_about = None, after_help = EXAMPLES)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

const EXAMPLES: &str = "\x1b[1mExamples:\x1b[0m
  echo \"hello world\" | mh encode base64
  echo \"aGVsbG8=\" | mh decode base64
  echo '{\"a\":1}' | mh fmt json
  mh gen guid
  mh gen timestamp-iso
  echo -n \"password\" | mh hash sha256
  cat data.yaml | mh convert yaml json
  echo \"#ff5500\" | mh color
  echo \"255\" | mh number
  echo \"hello\" | mh pipe base64_encode uppercase trim
  mh run \"My Custom Pipeline\" < input.txt
  mh roll 1d20
  mh roll 3d6+2
  mh password 20
  mh password 32 alphanum
  mh qr \"https://example.com\" > qr.png
  echo \"hello\" | mh qr > qr.png
  mh list
  mh secret set api_token
  mh secret set deploy_tmp --ttl 30m --from-clipboard
  mh secret list
  mh exec -e GH_TOKEN=github_pat -- gh api /user
  mh exec -e T=api_token -- sh -c 'curl -H \"Authorization: Bearer $T\" https://example.com'
  mh exec --env-file .mhenv -- npm start";

#[derive(Subcommand)]
enum Commands {
    /// Run a saved pipeline by name
    #[command(alias = "r")]
    Run {
        /// Pipeline name (as shown in the app)
        name: String,
    },

    /// Chain actions inline (stdin → action1 → action2 → ... → stdout)
    #[command(alias = "p")]
    Pipe {
        /// Action names to chain (e.g. base64_encode uppercase trim)
        actions: Vec<String>,
    },

    /// Generate a value (no stdin needed)
    #[command(alias = "g")]
    Gen {
        /// Type: guid, timestamp-iso, timestamp-unix, timestamp-utc
        #[arg(value_name = "TYPE")]
        kind: String,
    },

    /// Format/pretty-print (reads stdin)
    #[command(alias = "f")]
    Fmt {
        /// Type: json, xml, yaml
        #[arg(value_name = "TYPE")]
        kind: String,
    },

    /// Encode text (reads stdin)
    #[command(alias = "e")]
    Encode {
        /// Type: base64, url, hex
        #[arg(value_name = "TYPE")]
        kind: String,
    },

    /// Decode text (reads stdin)
    #[command(alias = "d")]
    Decode {
        /// Type: base64, url, hex, jwt, html
        #[arg(value_name = "TYPE")]
        kind: String,
    },

    /// Hash text (reads stdin)
    #[command(alias = "h")]
    Hash {
        /// Type: md5, sha1, sha256
        #[arg(value_name = "TYPE")]
        kind: String,
    },

    /// Convert between config formats (reads stdin)
    Convert {
        /// Source format: json, yaml, toml
        from: String,
        /// Target format: json, yaml, toml
        to: String,
    },

    /// Convert a color between formats (pass as argument or stdin)
    Color {
        /// Color value, e.g. #ff5500, rgb(255,0,0), hsl(180,50%,50%)
        value: Option<String>,
    },

    /// Convert number between bases (pass as argument or stdin)
    Number {
        /// Number value, e.g. 255, 0xff, 0b11111111, 0o377
        value: Option<String>,
    },

    /// Count characters, words, lines, bytes (reads stdin)
    Count,

    /// Convert Markdown to HTML (reads stdin)
    Md2html,

    /// Convert HTML to Markdown (reads stdin)
    Html2md,

    /// Generate lorem ipsum text
    Lorem {
        /// Spec: "50 words", "3 sentences", "2 paragraphs", or just a number
        #[arg(default_value = "50 words")]
        spec: Vec<String>,
    },

    /// Extract regex matches from stdin
    Regex {
        /// Regex pattern (use capture groups to extract specific parts)
        pattern: String,
    },

    /// Roll dice (e.g. 1d20, 3d6+2, 4d6)
    Roll {
        /// Dice notation: NdM or NdM±K (N defaults to 1)
        #[arg(value_name = "NOTATION")]
        spec: Vec<String>,
    },

    /// Generate a secure password (length + optional charset keyword)
    Password {
        /// Spec: "20", "32 alphanum", "16 hex", "12 numeric", "24 letters" (blank = 20 mixed)
        spec: Vec<String>,
    },

    /// Generate a QR code PNG (input as argument or stdin, PNG bytes to stdout)
    Qr {
        /// Text to encode; omit to read from stdin
        text: Option<String>,
    },

    /// Manage secrets in the OS keychain
    Secret {
        #[command(subcommand)]
        action: SecretAction,
    },

    /// Run a command with secrets as env vars; their values are masked in its output
    Exec {
        /// Inject a secret as an env var: VAR=secret_name (repeatable)
        #[arg(short = 'e', long = "env", value_name = "VAR=SECRET")]
        env: Vec<String>,
        /// File with one VAR=secret_name per line (# starts a comment)
        #[arg(long, value_name = "FILE")]
        env_file: Option<PathBuf>,
        /// Don't mask output; the command writes straight to the terminal
        #[arg(long)]
        no_mask: bool,
        /// Command and its arguments, after --
        #[arg(last = true, required = true, value_name = "COMMAND")]
        command: Vec<String>,
    },

    /// List all saved pipelines
    #[command(alias = "ls")]
    List,
}

#[derive(Subcommand)]
enum SecretAction {
    /// Print a secret's value (prefer `mh exec` when output is captured or logged)
    Get {
        /// Secret key name
        key: String,
    },
    /// Set a secret in the keychain (reads value from stdin, or --from-clipboard)
    Set {
        /// Secret key name
        key: String,
        /// Make it temporary: deleted after this long (e.g. 30m, 2h, 1d)
        #[arg(long, value_name = "DURATION")]
        ttl: Option<String>,
        /// Read the value from the clipboard, then clear the clipboard
        #[arg(long)]
        from_clipboard: bool,
    },
    /// Delete a secret from the keychain
    #[command(alias = "rm")]
    Delete {
        /// Secret key name
        key: String,
    },
    /// List secret names (never values)
    #[command(alias = "ls")]
    List,
    /// Delete all temporary secrets now
    Purge,
}

fn fail(msg: impl std::fmt::Display) -> ! {
    eprintln!("error: {}", msg);
    std::process::exit(1)
}

fn describe_expiry(at: u64) -> String {
    let local = chrono::DateTime::from_timestamp(at as i64, 0)
        .map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default();
    let left = at.saturating_sub(secrets::now());
    let left = match left {
        86400.. => format!("{}d {}h", left / 86400, left % 86400 / 3600),
        3600.. => format!("{}h {}m", left / 3600, left % 3600 / 60),
        60.. => format!("{}m", left / 60),
        _ => format!("{}s", left),
    };
    format!("{} (in {})", local, left)
}

fn set_secret(key: &str, ttl: Option<String>, from_clipboard: bool) {
    let ttl = ttl.map(|spec| secrets::parse_ttl(&spec).unwrap_or_else(|e| fail(e)));
    let mut clipboard = from_clipboard
        .then(|| arboard::Clipboard::new().unwrap_or_else(|e| fail(format!("cannot open clipboard: {}", e))));
    let raw = match clipboard.as_mut() {
        Some(cb) => cb.get_text().unwrap_or_default(),
        None => read_stdin(),
    };
    let value = raw.trim();
    if value.is_empty() {
        fail(if from_clipboard {
            "the clipboard has no text"
        } else {
            "no value provided (pipe it via stdin or use --from-clipboard)"
        });
    }
    match ttl {
        Some(ttl) => {
            let at = secrets::set_temp(key, value, ttl).unwrap_or_else(|e| fail(e));
            eprintln!("temporary secret '{}' stored, expires {}", key, describe_expiry(at));
        }
        None => {
            secrets::set(key, value).unwrap_or_else(|e| fail(e));
            eprintln!("secret '{}' stored", key);
        }
    }
    if let Some(cb) = clipboard.as_mut() {
        match cb.clear() {
            Ok(()) => eprintln!("clipboard cleared"),
            Err(e) => eprintln!("warning: could not clear the clipboard: {}", e),
        }
    }
}

#[cfg(windows)]
fn resolve_program(program: &str) -> PathBuf {
    let path = std::path::Path::new(program);
    if path.extension().is_some() || path.components().count() > 1 {
        return path.to_path_buf();
    }
    let dirs = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).filter(|d| d.is_absolute()).collect::<Vec<_>>())
        .unwrap_or_default();
    for dir in dirs {
        for ext in ["com", "exe", "bat", "cmd"] {
            let candidate = dir.join(program).with_extension(ext);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    path.to_path_buf()
}

#[cfg(not(windows))]
fn resolve_program(program: &str) -> PathBuf {
    PathBuf::from(program)
}

// Keeps draining after the destination closes so the child never blocks on a full pipe.
fn pump(mut src: impl Read, mut dst: impl Write, mut masker: Masker) {
    let mut buf = [0u8; 8192];
    let mut open = true;
    loop {
        let n = match src.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        let masked = masker.push(&buf[..n]);
        if open {
            open = dst.write_all(&masked).and_then(|_| dst.flush()).is_ok();
        }
    }
    if open {
        dst.write_all(&masker.finish()).and_then(|_| dst.flush()).ok();
    }
}

fn exec(env: Vec<String>, env_file: Option<PathBuf>, no_mask: bool, command: Vec<String>) -> Result<i32, String> {
    let mut specs = Vec::new();
    if let Some(path) = env_file {
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
        specs.extend(
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(String::from),
        );
    }
    specs.extend(env);

    let mut vars = Vec::new();
    let mut named = Vec::new();
    for spec in &specs {
        let (var, name) = spec
            .split_once('=')
            .map(|(v, n)| (v.trim(), n.trim()))
            .filter(|(v, n)| !v.is_empty() && !n.is_empty())
            .ok_or_else(|| format!("expected VAR=secret_name, got '{}'", spec))?;
        let value = secrets::get(name)?;
        vars.push((var.to_string(), value.clone()));
        named.push((name.to_string(), value));
    }

    let mut cmd = Command::new(resolve_program(&command[0]));
    cmd.args(&command[1..]).envs(vars);
    let spawn_err = |e: io::Error| format!("failed to run '{}': {}", command[0], e);

    if no_mask {
        let status = cmd.status().map_err(spawn_err)?;
        return Ok(status.code().unwrap_or(1));
    }

    let mask_out = Masker::new(&named)?;
    let mask_err = Masker::new(&named)?;
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(spawn_err)?;
    let out = child.stdout.take().ok_or("child stdout unavailable")?;
    let err = child.stderr.take().ok_or("child stderr unavailable")?;
    let t_out = std::thread::spawn(move || pump(out, io::stdout(), mask_out));
    let t_err = std::thread::spawn(move || pump(err, io::stderr(), mask_err));
    let status = child.wait().map_err(|e| format!("failed waiting for '{}': {}", command[0], e))?;
    t_out.join().ok();
    t_err.join().ok();
    Ok(status.code().unwrap_or(1))
}

fn read_stdin() -> String {
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf).unwrap_or_default();
    buf
}

fn run_and_print(action: &str, input: &str, key: Option<&str>) {
    match run_action(action, input, key) {
        Ok(result) => print!("{}", result),
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { name } => {
            let cmds = load_commands();
            let cmd = cmds.iter().find(|c| c.name == name);
            match cmd {
                Some(cmd) => {
                    let steps = &cmd.steps;
                    if steps.is_empty() {
                        eprintln!("error: pipeline '{}' has no steps", name);
                        std::process::exit(1);
                    }

                    let first = &steps[0];
                    let mut value = if magic_hotkey_lib::is_generator(&first.action) {
                        match run_action(&first.action, "", first.key.as_deref()) {
                            Ok(v) => v,
                            Err(e) => { eprintln!("error: {}", e); std::process::exit(1); }
                        }
                    } else {
                        let input = read_stdin();
                        match run_action(&first.action, &input, first.key.as_deref()) {
                            Ok(v) => v,
                            Err(e) => { eprintln!("error: {}", e); std::process::exit(1); }
                        }
                    };

                    for s in &steps[1..] {
                        match run_action(&s.action, &value, s.key.as_deref()) {
                            Ok(v) => value = v,
                            Err(e) => { eprintln!("error: {}", e); std::process::exit(1); }
                        }
                    }
                    print!("{}", value);
                }
                None => {
                    eprintln!("error: pipeline '{}' not found", name);
                    eprintln!("available pipelines:");
                    for c in load_commands() {
                        eprintln!("  {}", c.name);
                    }
                    std::process::exit(1);
                }
            }
        }

        Commands::Pipe { actions } => {
            if actions.is_empty() {
                eprintln!("error: provide at least one action");
                std::process::exit(1);
            }

            let first = &actions[0];
            let mut value = if magic_hotkey_lib::is_generator(first) {
                match run_action(first, "", None) {
                    Ok(v) => v,
                    Err(e) => { eprintln!("error: {}", e); std::process::exit(1); }
                }
            } else {
                let input = read_stdin();
                match run_action(first, &input, None) {
                    Ok(v) => v,
                    Err(e) => { eprintln!("error: {}", e); std::process::exit(1); }
                }
            };

            for action in &actions[1..] {
                match run_action(action, &value, None) {
                    Ok(v) => value = v,
                    Err(e) => { eprintln!("error: {}", e); std::process::exit(1); }
                }
            }
            print!("{}", value);
        }

        Commands::Gen { kind } => {
            let action = match kind.as_str() {
                "guid" | "uuid" => "generate_guid",
                "timestamp-iso" | "iso" => "timestamp_iso",
                "timestamp-unix" | "unix" => "timestamp_unix",
                "timestamp-utc" | "utc" => "timestamp_utc",
                other => {
                    eprintln!("error: unknown generator '{}'. Options: guid, timestamp-iso, timestamp-unix, timestamp-utc", other);
                    std::process::exit(1);
                }
            };
            run_and_print(action, "", None);
        }

        Commands::Fmt { kind } => {
            let action = match kind.as_str() {
                "json" => "format_json",
                "xml" => "format_xml",
                "yaml" | "yml" => "format_yaml",
                other => {
                    eprintln!("error: unknown format '{}'. Options: json, xml, yaml", other);
                    std::process::exit(1);
                }
            };
            let input = read_stdin();
            run_and_print(action, &input, None);
        }

        Commands::Encode { kind } => {
            let action = match kind.as_str() {
                "base64" | "b64" => "base64_encode",
                "url" => "url_encode",
                "hex" => "hex_encode",
                other => {
                    eprintln!("error: unknown encoding '{}'. Options: base64, url, hex", other);
                    std::process::exit(1);
                }
            };
            let input = read_stdin();
            run_and_print(action, &input, None);
        }

        Commands::Decode { kind } => {
            let action = match kind.as_str() {
                "base64" | "b64" => "base64_decode",
                "url" => "url_decode",
                "hex" => "hex_decode",
                "jwt" => "jwt_decode",
                "html" => "html_decode",
                other => {
                    eprintln!("error: unknown decoding '{}'. Options: base64, url, hex, jwt, html", other);
                    std::process::exit(1);
                }
            };
            let input = read_stdin();
            run_and_print(action, &input, None);
        }

        Commands::Hash { kind } => {
            let action = match kind.as_str() {
                "md5" => "hash_md5",
                "sha1" => "hash_sha1",
                "sha256" | "sha" => "hash_sha256",
                other => {
                    eprintln!("error: unknown hash '{}'. Options: md5, sha1, sha256", other);
                    std::process::exit(1);
                }
            };
            let input = read_stdin();
            run_and_print(action, &input, None);
        }

        Commands::Convert { from, to } => {
            let action = match (from.as_str(), to.as_str()) {
                ("json", "yaml" | "yml") => "json_to_yaml",
                ("json", "toml") => "json_to_toml",
                ("yaml" | "yml", "json") => "yaml_to_json",
                ("toml", "json") => "toml_to_json",
                _ => {
                    eprintln!("error: unsupported conversion '{}' → '{}'. Options: json↔yaml, json↔toml, yaml→json, toml→json", from, to);
                    std::process::exit(1);
                }
            };
            let input = read_stdin();
            run_and_print(action, &input, None);
        }

        Commands::Color { value } => {
            let input = value.unwrap_or_else(|| read_stdin().trim().to_string());
            run_and_print("color_convert", &input, None);
        }

        Commands::Number { value } => {
            let input = value.unwrap_or_else(|| read_stdin().trim().to_string());
            run_and_print("number_convert", &input, None);
        }

        Commands::Count => {
            let input = read_stdin();
            // Parse the JSON result and display it nicely
            match commands::count(&input) {
                Ok(json) => {
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(&json) {
                        println!("Characters:           {}", data["characters"]);
                        println!("Characters (no space): {}", data["characters_no_spaces"]);
                        println!("Words:                {}", data["words"]);
                        println!("Lines:                {}", data["lines"]);
                        println!("Bytes:                {}", data["bytes"]);
                    } else {
                        print!("{}", json);
                    }
                }
                Err(e) => { eprintln!("error: {}", e); std::process::exit(1); }
            }
        }

        Commands::Md2html => {
            let input = read_stdin();
            run_and_print("md_to_html", &input, None);
        }

        Commands::Html2md => {
            let input = read_stdin();
            run_and_print("html_to_md", &input, None);
        }

        Commands::Lorem { spec } => {
            let spec_str = spec.join(" ");
            run_and_print("lorem_ipsum", "", Some(&spec_str));
        }

        Commands::Regex { pattern } => {
            let input = read_stdin();
            run_and_print("regex_extract", &input, Some(&pattern));
        }

        Commands::Roll { spec } => {
            let spec_str = spec.join("");
            if spec_str.is_empty() {
                eprintln!("error: provide dice notation (e.g. 1d20, 3d6+2)");
                std::process::exit(1);
            }
            match commands::roll_dice(&spec_str) {
                Ok(json) => {
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(&json) {
                        println!("Rolls: {}", data["rolls"].as_str().unwrap_or(""));
                        println!("Total: {}", data["total"]);
                    } else {
                        print!("{}", json);
                    }
                }
                Err(e) => { eprintln!("error: {}", e); std::process::exit(1); }
            }
        }

        Commands::Password { spec } => {
            let spec_str = spec.join(" ");
            run_and_print("password", "", Some(&spec_str));
        }

        Commands::Qr { text } => {
            let input = text.unwrap_or_else(|| read_stdin().trim().to_string());
            match commands::qr_code_png(&input) {
                Ok(bytes) => {
                    use std::io::Write;
                    if let Err(e) = std::io::stdout().write_all(&bytes) {
                        eprintln!("error: failed to write PNG to stdout: {}", e);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("error: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Commands::Secret { action } => {
            secrets::sweep_expired().ok();
            match action {
                SecretAction::Get { key } => {
                    run_and_print("secret", "", Some(&key));
                }
                SecretAction::Set { key, ttl, from_clipboard } => {
                    set_secret(&key, ttl, from_clipboard);
                }
                SecretAction::Delete { key } => {
                    secrets::delete(&key).unwrap_or_else(|e| fail(e));
                    eprintln!("secret '{}' deleted", key);
                }
                SecretAction::List => {
                    let list = secrets::list().unwrap_or_else(|e| fail(e));
                    if list.is_empty() {
                        eprintln!("no secrets stored");
                    }
                    for s in list {
                        match s.expires_at {
                            None => println!("{}", s.name),
                            Some(at) => println!("{:30} temporary, expires {}", s.name, describe_expiry(at)),
                        }
                    }
                }
                SecretAction::Purge => {
                    let n = secrets::purge_temp().unwrap_or_else(|e| fail(e));
                    eprintln!("deleted {} temporary secret(s)", n);
                }
            }
        }

        Commands::Exec { env, env_file, no_mask, command } => {
            secrets::sweep_expired().ok();
            match exec(env, env_file, no_mask, command) {
                Ok(code) => std::process::exit(code),
                Err(e) => fail(e),
            }
        }

        Commands::List => {
            let cmds = load_commands();
            if cmds.is_empty() {
                println!("No pipelines configured.");
            } else {
                for cmd in &cmds {
                    let steps: Vec<String> = cmd.steps.iter().map(|s| {
                        let mut label = s.action.clone();
                        if let Some(ref k) = s.key { label.push_str(&format!(":{}", k)); }
                        if let Some(ref t) = s.template {
                            let preview = if t.len() > 30 { format!("{}...", &t[..30]) } else { t.clone() };
                            label = format!("snippet({})", preview);
                        }
                        label
                    }).collect();
                    println!("  {:30} {}", cmd.name, steps.join(" → "));
                }
            }
        }
    }
}

use clap::{Parser, Subcommand};
use magic_hotkey_lib::{commands, load_commands, run_action};
use std::io::{self, Read};

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
  mh list
  mh secret set api_token
  mh secret get api_token";

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

    /// Manage secrets in the OS keychain
    Secret {
        #[command(subcommand)]
        action: SecretAction,
    },

    /// List all saved pipelines
    #[command(alias = "ls")]
    List,
}

#[derive(Subcommand)]
enum SecretAction {
    /// Get a secret from the keychain
    Get {
        /// Secret key name
        key: String,
    },
    /// Set a secret in the keychain (reads value from stdin)
    Set {
        /// Secret key name
        key: String,
    },
    /// Delete a secret from the keychain
    #[command(alias = "rm")]
    Delete {
        /// Secret key name
        key: String,
    },
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

        Commands::Secret { action } => match action {
            SecretAction::Get { key } => {
                run_and_print("secret", "", Some(&key));
            }
            SecretAction::Set { key } => {
                let value = read_stdin().trim().to_string();
                if value.is_empty() {
                    eprintln!("error: no value provided (pipe the value via stdin)");
                    std::process::exit(1);
                }
                match commands::set_secret(&key, &value) {
                    Ok(_) => eprintln!("secret '{}' stored", key),
                    Err(e) => { eprintln!("error: {}", e); std::process::exit(1); }
                }
            }
            SecretAction::Delete { key } => {
                match commands::delete_secret(&key) {
                    Ok(_) => eprintln!("secret '{}' deleted", key),
                    Err(e) => { eprintln!("error: {}", e); std::process::exit(1); }
                }
            }
        },

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

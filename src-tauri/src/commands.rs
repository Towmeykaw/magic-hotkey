use base64::{engine::general_purpose, Engine};
use keyring::Entry;
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::SERVICE_NAME;

pub fn generate_guid() -> Result<String, String> {
    Ok(Uuid::new_v4().to_string())
}

pub fn format_json(input: &str) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|e| format!("Invalid JSON: {}", e))?;
    serde_json::to_string_pretty(&value).map_err(|e| format!("Failed to format JSON: {}", e))
}

pub fn base64_encode(input: &str) -> Result<String, String> {
    Ok(general_purpose::STANDARD.encode(input.as_bytes()))
}

pub fn base64_decode(input: &str) -> Result<String, String> {
    let bytes = general_purpose::STANDARD
        .decode(input.trim())
        .map_err(|e| format!("Invalid Base64: {}", e))?;
    String::from_utf8(bytes).map_err(|e| format!("Decoded bytes are not valid UTF-8: {}", e))
}

pub fn url_encode(input: &str) -> Result<String, String> {
    Ok(urlencoding::encode(input).into_owned())
}

pub fn url_decode(input: &str) -> Result<String, String> {
    urlencoding::decode(input)
        .map(|s| s.into_owned())
        .map_err(|e| format!("Invalid URL encoding: {}", e))
}

pub fn jwt_decode(input: &str) -> Result<String, String> {
    let token = input.trim();
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 {
        return Err("Invalid JWT: expected at least 2 dot-separated parts".to_string());
    }

    let decode_part = |part: &str, label: &str| -> Result<serde_json::Value, String> {
        // JWT uses base64url (no padding), so fix up for standard base64
        let padded = match part.len() % 4 {
            2 => format!("{}==", part),
            3 => format!("{}=", part),
            _ => part.to_string(),
        };
        let replaced = padded.replace('-', "+").replace('_', "/");
        let bytes = general_purpose::STANDARD
            .decode(&replaced)
            .map_err(|e| format!("Failed to decode {} base64: {}", label, e))?;
        serde_json::from_slice(&bytes)
            .map_err(|e| format!("Failed to parse {} JSON: {}", label, e))
    };

    let header = decode_part(parts[0], "header")?;
    let payload = decode_part(parts[1], "payload")?;

    let result = serde_json::json!({
        "header": header,
        "payload": payload
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialize error: {}", e))
}

pub fn hex_encode(input: &str) -> Result<String, String> {
    Ok(input.bytes().map(|b| format!("{:02x}", b)).collect::<String>())
}

pub fn hex_decode(input: &str) -> Result<String, String> {
    let hex = input.trim().strip_prefix("0x").unwrap_or(input.trim());
    let hex_clean: String = hex.chars().filter(|c| !c.is_whitespace()).collect();

    if hex_clean.len() % 2 != 0 {
        return Err("Hex string must have even number of characters".to_string());
    }

    let bytes: Result<Vec<u8>, _> = (0..hex_clean.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex_clean[i..i + 2], 16))
        .collect();

    let bytes = bytes.map_err(|e| format!("Invalid hex: {}", e))?;
    String::from_utf8(bytes).map_err(|e| format!("Decoded bytes are not valid UTF-8: {}", e))
}

pub fn html_decode(input: &str) -> Result<String, String> {
    let result = input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&#x27;", "'")
        .replace("&#x2F;", "/")
        .replace("&nbsp;", "\u{00A0}")
        .replace("&#10;", "\n")
        .replace("&#13;", "\r")
        .replace("&#9;", "\t");

    // Handle numeric entities like &#123; and &#x1F4A9;
    let mut output = String::new();
    let mut chars = result.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' && chars.peek() == Some(&'#') {
            chars.next(); // consume '#'
            let mut num_str = String::new();
            let is_hex = chars.peek() == Some(&'x') || chars.peek() == Some(&'X');
            if is_hex {
                chars.next(); // consume 'x'
            }
            while let Some(&nc) = chars.peek() {
                if nc == ';' {
                    chars.next();
                    break;
                }
                num_str.push(nc);
                chars.next();
            }
            let code_point = if is_hex {
                u32::from_str_radix(&num_str, 16).ok()
            } else {
                num_str.parse::<u32>().ok()
            };
            if let Some(cp) = code_point.and_then(char::from_u32) {
                output.push(cp);
            } else {
                output.push('&');
                output.push('#');
                if is_hex {
                    output.push('x');
                }
                output.push_str(&num_str);
                output.push(';');
            }
        } else {
            output.push(c);
        }
    }
    Ok(output)
}

// ── Markdown ↔ HTML ─────────────────────────────────────────────────

pub fn markdown_to_html(input: &str) -> Result<String, String> {
    let parser = pulldown_cmark::Parser::new(input);
    let mut html_output = String::new();
    pulldown_cmark::html::push_html(&mut html_output, parser);
    Ok(html_output.trim().to_string())
}

pub fn html_to_markdown(input: &str) -> Result<String, String> {
    let mut result = String::new();
    let mut tag_stack: Vec<String> = Vec::new();
    let mut list_depth: usize = 0;
    let mut ol_counters: Vec<usize> = Vec::new();
    let mut in_pre = false;
    let mut in_code = false;
    let mut link_href: Option<String> = None;

    // Normalize <br> and <hr> self-closing tags
    let normalized = input
        .replace("<br>", "<br/>")
        .replace("<br />", "<br/>")
        .replace("<hr>", "<hr/>")
        .replace("<hr />", "<hr/>");

    let wrapped = format!("<root>{}</root>", normalized);
    let mut reader = quick_xml::Reader::from_str(&wrapped);
    reader.config_mut().trim_text(false);

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(quick_xml::events::Event::Start(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_lowercase();
                match tag.as_str() {
                    "h1" => result.push_str("# "),
                    "h2" => result.push_str("## "),
                    "h3" => result.push_str("### "),
                    "h4" => result.push_str("#### "),
                    "h5" => result.push_str("##### "),
                    "h6" => result.push_str("###### "),
                    "strong" | "b" => result.push_str("**"),
                    "em" | "i" => result.push('*'),
                    "code" if !in_pre => { result.push('`'); in_code = true; }
                    "pre" => { result.push_str("```\n"); in_pre = true; }
                    "a" => {
                        let mut href = String::new();
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"href" {
                                href = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                        link_href = Some(href);
                        result.push('[');
                    }
                    "ul" => { list_depth += 1; }
                    "ol" => { list_depth += 1; ol_counters.push(0); }
                    "li" => {
                        let indent = "  ".repeat(list_depth.saturating_sub(1));
                        if let Some(counter) = ol_counters.last_mut().filter(|_| tag_stack.last().map(|s| s.as_str()) != Some("ul")) {
                            *counter += 1;
                            result.push_str(&format!("{}{}. ", indent, counter));
                        } else {
                            result.push_str(&format!("{}- ", indent));
                        }
                    }
                    "blockquote" => result.push_str("> "),
                    "p" | "div" => {}
                    "img" => {
                        let mut src = String::new();
                        let mut alt = String::new();
                        for attr in e.attributes().flatten() {
                            match attr.key.as_ref() {
                                b"src" => src = String::from_utf8_lossy(&attr.value).to_string(),
                                b"alt" => alt = String::from_utf8_lossy(&attr.value).to_string(),
                                _ => {}
                            }
                        }
                        result.push_str(&format!("![{}]({})", alt, src));
                    }
                    _ => {}
                }
                tag_stack.push(tag);
            }
            Ok(quick_xml::events::Event::End(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_lowercase();
                match tag.as_str() {
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => result.push_str("\n\n"),
                    "strong" | "b" => result.push_str("**"),
                    "em" | "i" => result.push('*'),
                    "code" if !in_pre => { result.push('`'); in_code = false; }
                    "pre" => { in_pre = false; result.push_str("\n```\n\n"); }
                    "a" => {
                        let href = link_href.take().unwrap_or_default();
                        result.push_str(&format!("]({})", href));
                    }
                    "ul" => { list_depth = list_depth.saturating_sub(1); if result.ends_with('\n') { result.push('\n'); } else { result.push_str("\n\n"); } }
                    "ol" => { list_depth = list_depth.saturating_sub(1); ol_counters.pop(); if result.ends_with('\n') { result.push('\n'); } else { result.push_str("\n\n"); } }
                    "li" => { if !result.ends_with('\n') { result.push('\n'); } }
                    "p" | "div" => result.push_str("\n\n"),
                    "blockquote" => result.push_str("\n\n"),
                    "root" => {}
                    _ => {}
                }
                tag_stack.pop();
            }
            Ok(quick_xml::events::Event::Empty(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_lowercase();
                match tag.as_str() {
                    "br" => result.push('\n'),
                    "hr" => result.push_str("\n---\n\n"),
                    "img" => {
                        let mut src = String::new();
                        let mut alt = String::new();
                        for attr in e.attributes().flatten() {
                            match attr.key.as_ref() {
                                b"src" => src = String::from_utf8_lossy(&attr.value).to_string(),
                                b"alt" => alt = String::from_utf8_lossy(&attr.value).to_string(),
                                _ => {}
                            }
                        }
                        result.push_str(&format!("![{}]({})", alt, src));
                    }
                    _ => {}
                }
            }
            Ok(quick_xml::events::Event::Text(ref e)) => {
                let text = e.unescape().unwrap_or_default();
                if in_pre || in_code {
                    result.push_str(&text);
                } else {
                    // Collapse whitespace for normal text
                    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
                    result.push_str(&collapsed);
                }
            }
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    // Clean up: remove excessive newlines
    let cleaned = regex::Regex::new(r"\n{3,}")
        .expect("hardcoded regex is valid")
        .replace_all(result.trim(), "\n\n");
    Ok(cleaned.to_string())
}

// ── Lorem ipsum ─────────────────────────────────────────────────────

const LOREM_WORDS: &[&str] = &[
    "lorem", "ipsum", "dolor", "sit", "amet", "consectetur", "adipiscing", "elit",
    "sed", "do", "eiusmod", "tempor", "incididunt", "ut", "labore", "et", "dolore",
    "magna", "aliqua", "enim", "ad", "minim", "veniam", "quis", "nostrud",
    "exercitation", "ullamco", "laboris", "nisi", "aliquip", "ex", "ea", "commodo",
    "consequat", "duis", "aute", "irure", "in", "reprehenderit", "voluptate",
    "velit", "esse", "cillum", "fugiat", "nulla", "pariatur", "excepteur", "sint",
    "occaecat", "cupidatat", "non", "proident", "sunt", "culpa", "qui", "officia",
    "deserunt", "mollit", "anim", "id", "est", "laborum", "at", "vero", "eos",
    "accusamus", "iusto", "odio", "dignissimos", "ducimus", "blanditiis",
    "praesentium", "voluptatum", "deleniti", "atque", "corrupti", "quos", "dolores",
    "quas", "molestias", "recusandae", "itaque", "earum", "rerum", "hic", "tenetur",
    "sapiente", "delectus", "aut", "reiciendis", "voluptatibus", "maiores", "alias",
    "perferendis", "doloribus", "asperiores", "repellat",
];

pub fn lorem_ipsum(input: &str) -> Result<String, String> {
    let trimmed = input.trim().to_lowercase();

    // Parse "5 words", "3 sentences", "2 paragraphs", or just a number (defaults to words)
    // Parse count + unit from spec. Check multi-char suffixes before single-char to avoid "10 words" matching "s".
    let (count, unit) = if let Some(rest) = trimmed.strip_suffix("paragraphs").or_else(|| trimmed.strip_suffix("paragraph")) {
        (rest.trim().parse::<usize>().unwrap_or(1), "paragraphs")
    } else if trimmed.ends_with('p') && !trimmed.ends_with("help") {
        let rest = &trimmed[..trimmed.len()-1];
        (rest.trim().parse::<usize>().unwrap_or(1), "paragraphs")
    } else if let Some(rest) = trimmed.strip_suffix("sentences").or_else(|| trimmed.strip_suffix("sentence")) {
        (rest.trim().parse::<usize>().unwrap_or(1), "sentences")
    } else if let Some(rest) = trimmed.strip_suffix("words").or_else(|| trimmed.strip_suffix("word")) {
        (rest.trim().parse::<usize>().unwrap_or(10), "words")
    } else if trimmed.ends_with('w') {
        let rest = &trimmed[..trimmed.len()-1];
        (rest.trim().parse::<usize>().unwrap_or(10), "words")
    } else if trimmed.ends_with('s') && trimmed.len() > 1 {
        let rest = &trimmed[..trimmed.len()-1];
        if let Ok(n) = rest.trim().parse::<usize>() { (n, "sentences") } else { (50, "words") }
    } else if let Ok(n) = trimmed.parse::<usize>() {
        (n, "words")
    } else {
        // Default: 50 words
        (50, "words")
    };

    let count = count.max(1);

    match unit {
        "words" => {
            let words: Vec<&str> = LOREM_WORDS.iter().cycle().take(count).copied().collect();
            let mut text = words.join(" ");
            // Capitalize first letter
            if let Some(first) = text.get_mut(..1) {
                first.make_ascii_uppercase();
            }
            text.push('.');
            Ok(text)
        }
        "sentences" => {
            let mut result = Vec::new();
            let mut word_idx = 0;
            for _ in 0..count {
                let len = 8 + (word_idx % 7); // vary sentence length 8-14 words
                let words: Vec<&str> = LOREM_WORDS.iter().cycle().skip(word_idx).take(len).copied().collect();
                word_idx += len;
                let mut sentence = words.join(" ");
                if let Some(first) = sentence.get_mut(..1) {
                    first.make_ascii_uppercase();
                }
                sentence.push('.');
                result.push(sentence);
            }
            Ok(result.join(" "))
        }
        "paragraphs" => {
            let mut result = Vec::new();
            let mut word_idx = 0;
            for _ in 0..count {
                let mut sentences = Vec::new();
                let sentence_count = 4 + (word_idx % 3); // 4-6 sentences per paragraph
                for _ in 0..sentence_count {
                    let len = 8 + (word_idx % 7);
                    let words: Vec<&str> = LOREM_WORDS.iter().cycle().skip(word_idx).take(len).copied().collect();
                    word_idx += len;
                    let mut sentence = words.join(" ");
                    if let Some(first) = sentence.get_mut(..1) {
                        first.make_ascii_uppercase();
                    }
                    sentence.push('.');
                    sentences.push(sentence);
                }
                result.push(sentences.join(" "));
            }
            Ok(result.join("\n\n"))
        }
        _ => unreachable!(),
    }
}

// ── Dice roller ─────────────────────────────────────────────────────

pub fn roll_dice(spec: &str) -> Result<String, String> {
    let trimmed = spec.trim().to_lowercase();
    let re = regex::Regex::new(r"^(\d*)d(\d+)\s*(?:([+-])\s*(\d+))?$")
        .expect("hardcoded regex is valid");
    let caps = re.captures(&trimmed).ok_or_else(|| {
        format!("Invalid dice notation '{}'. Expected NdM or NdM±K (e.g. 1d20, 3d6+2)", spec)
    })?;

    let n_str = caps.get(1).map(|m| m.as_str()).unwrap_or("");
    let n: u32 = if n_str.is_empty() { 1 } else {
        n_str.parse().map_err(|_| format!("Invalid dice count: '{}'", n_str))?
    };
    let sides: u32 = caps.get(2).unwrap().as_str().parse()
        .map_err(|_| "Invalid number of sides".to_string())?;
    let modifier: i32 = match (caps.get(3), caps.get(4)) {
        (Some(sign), Some(num)) => {
            let val: i32 = num.as_str().parse()
                .map_err(|_| "Invalid modifier".to_string())?;
            if sign.as_str() == "-" { -val } else { val }
        }
        _ => 0,
    };

    if n == 0 { return Err("Number of dice must be at least 1".into()); }
    if n > 100 { return Err("Number of dice too high (max 100)".into()); }
    if sides < 2 { return Err("Sides must be at least 2".into()); }
    if sides > 1000 { return Err("Sides too high (max 1000)".into()); }

    use rand::Rng;
    let mut rng = rand::thread_rng();
    let rolls: Vec<u32> = (0..n).map(|_| rng.gen_range(1..=sides)).collect();
    let roll_sum: i32 = rolls.iter().map(|&r| r as i32).sum();
    let total = roll_sum + modifier;

    let rolls_str = rolls.iter().map(|r| r.to_string()).collect::<Vec<_>>().join(", ");
    let display = match modifier.cmp(&0) {
        std::cmp::Ordering::Greater => format!("[{}] + {}", rolls_str, modifier),
        std::cmp::Ordering::Less => format!("[{}] - {}", rolls_str, modifier.abs()),
        std::cmp::Ordering::Equal => format!("[{}]", rolls_str),
    };

    let result = serde_json::json!({
        "total": total,
        "rolls": display,
    });
    serde_json::to_string(&result).map_err(|e| format!("Serialize error: {}", e))
}

// ── Regex extract ───────────────────────────────────────────────────

pub fn regex_extract(input: &str, pattern: &str) -> Result<String, String> {
    let re = regex::Regex::new(pattern).map_err(|e| format!("Invalid regex: {}", e))?;

    let matches: Vec<String> = re.find_iter(input).map(|m| m.as_str().to_string()).collect();

    if matches.is_empty() {
        return Err(format!("No matches found for pattern: {}", pattern));
    }

    // If there are capture groups, extract those instead
    let caps: Vec<String> = re.captures_iter(input).flat_map(|cap| {
        // Skip group 0 (full match) if there are named/numbered groups
        if cap.len() > 1 {
            (1..cap.len()).filter_map(|i| cap.get(i).map(|m| m.as_str().to_string())).collect::<Vec<_>>()
        } else {
            vec![cap[0].to_string()]
        }
    }).collect();

    Ok(caps.join("\n"))
}

// ── Number base converter ────────────────────────────────────────────

pub fn number_convert(input: &str) -> Result<String, String> {
    let trimmed = input.trim();

    // Try hex (0x prefix)
    if let Some(hex) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
        let n = u64::from_str_radix(hex, 16).map_err(|e| format!("Invalid hex: {}", e))?;
        return Ok(format!("Decimal:  {}\nBinary:   0b{:b}\nOctal:    0o{:o}\nHex:      0x{:x}", n, n, n, n));
    }

    // Try binary (0b prefix)
    if let Some(bin) = trimmed.strip_prefix("0b").or_else(|| trimmed.strip_prefix("0B")) {
        let n = u64::from_str_radix(bin, 2).map_err(|e| format!("Invalid binary: {}", e))?;
        return Ok(format!("Decimal:  {}\nBinary:   0b{:b}\nOctal:    0o{:o}\nHex:      0x{:x}", n, n, n, n));
    }

    // Try octal (0o prefix)
    if let Some(oct) = trimmed.strip_prefix("0o").or_else(|| trimmed.strip_prefix("0O")) {
        let n = u64::from_str_radix(oct, 8).map_err(|e| format!("Invalid octal: {}", e))?;
        return Ok(format!("Decimal:  {}\nBinary:   0b{:b}\nOctal:    0o{:o}\nHex:      0x{:x}", n, n, n, n));
    }

    // Default: decimal
    let n: u64 = trimmed.parse().map_err(|e| format!("Invalid number: {}", e))?;
    Ok(format!("Decimal:  {}\nBinary:   0b{:b}\nOctal:    0o{:o}\nHex:      0x{:x}", n, n, n, n))
}

// ── Color converter ─────────────────────────────────────────────────

pub fn color_convert(input: &str) -> Result<String, String> {
    let trimmed = input.trim();

    // Try hex color: #RGB, #RRGGBB
    if let Some(hex) = trimmed.strip_prefix('#') {
        let (r, g, b) = match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).map_err(|_| "Invalid hex color")?;
                let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).map_err(|_| "Invalid hex color")?;
                let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).map_err(|_| "Invalid hex color")?;
                (r, g, b)
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| "Invalid hex color")?;
                let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| "Invalid hex color")?;
                let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| "Invalid hex color")?;
                (r, g, b)
            }
            _ => return Err("Hex color must be #RGB or #RRGGBB".into()),
        };
        let (h, s, l) = rgb_to_hsl(r, g, b);
        return Ok(format!("HEX:  #{:02x}{:02x}{:02x}\nRGB:  rgb({}, {}, {})\nHSL:  hsl({}, {}%, {}%)", r, g, b, r, g, b, h, s, l));
    }

    // Try rgb(r, g, b)
    if trimmed.starts_with("rgb(") || trimmed.starts_with("RGB(") {
        let inner = trimmed
            .trim_start_matches(|c: char| c.is_alphabetic() || c == '(')
            .trim_end_matches(')');
        let parts: Vec<u8> = inner.split(',')
            .map(|s| s.trim().parse().map_err(|_| "Invalid RGB value"))
            .collect::<Result<Vec<_>, _>>()?;
        if parts.len() != 3 { return Err("RGB needs 3 values".into()); }
        let (r, g, b) = (parts[0], parts[1], parts[2]);
        let (h, s, l) = rgb_to_hsl(r, g, b);
        return Ok(format!("HEX:  #{:02x}{:02x}{:02x}\nRGB:  rgb({}, {}, {})\nHSL:  hsl({}, {}%, {}%)", r, g, b, r, g, b, h, s, l));
    }

    // Try hsl(h, s%, l%)
    if trimmed.starts_with("hsl(") || trimmed.starts_with("HSL(") {
        let inner = trimmed
            .trim_start_matches(|c: char| c.is_alphabetic() || c == '(')
            .trim_end_matches(')');
        let parts: Vec<f64> = inner.split(',')
            .map(|s| s.trim().trim_end_matches('%').parse().map_err(|_| "Invalid HSL value"))
            .collect::<Result<Vec<_>, _>>()?;
        if parts.len() != 3 { return Err("HSL needs 3 values".into()); }
        let (r, g, b) = hsl_to_rgb(parts[0] as u16, parts[1] as u8, parts[2] as u8);
        let (h, s, l) = (parts[0] as u16, parts[1] as u8, parts[2] as u8);
        return Ok(format!("HEX:  #{:02x}{:02x}{:02x}\nRGB:  rgb({}, {}, {})\nHSL:  hsl({}, {}%, {}%)", r, g, b, r, g, b, h, s, l));
    }

    Err("Unrecognized color format. Try #RRGGBB, rgb(r,g,b), or hsl(h,s%,l%)".into())
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (u16, u8, u8) {
    let r = r as f64 / 255.0;
    let g = g as f64 / 255.0;
    let b = b as f64 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    if (max - min).abs() < f64::EPSILON {
        return (0, 0, (l * 100.0).round() as u8);
    }

    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if (max - r).abs() < f64::EPSILON {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
    } else if (max - g).abs() < f64::EPSILON {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };

    ((h * 360.0).round() as u16, (s * 100.0).round() as u8, (l * 100.0).round() as u8)
}

fn hsl_to_rgb(h: u16, s: u8, l: u8) -> (u8, u8, u8) {
    let s = s as f64 / 100.0;
    let l = l as f64 / 100.0;

    if s == 0.0 {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }

    let h = h as f64 / 360.0;
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;

    let hue_to_rgb = |p: f64, q: f64, mut t: f64| -> f64 {
        if t < 0.0 { t += 1.0; }
        if t > 1.0 { t -= 1.0; }
        if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
        if t < 1.0 / 2.0 { return q; }
        if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
        p
    };

    let r = (hue_to_rgb(p, q, h + 1.0 / 3.0) * 255.0).round() as u8;
    let g = (hue_to_rgb(p, q, h) * 255.0).round() as u8;
    let b = (hue_to_rgb(p, q, h - 1.0 / 3.0) * 255.0).round() as u8;
    (r, g, b)
}

// ── Config format converters ────────────────────────────────────────

pub fn json_to_yaml(input: &str) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(input.trim()).map_err(|e| format!("Invalid JSON: {}", e))?;
    serde_yaml::to_string(&value).map_err(|e| format!("YAML conversion error: {}", e))
}

pub fn json_to_toml(input: &str) -> Result<String, String> {
    let json: serde_json::Value = serde_json::from_str(input.trim())
        .map_err(|e| format!("Invalid JSON: {}", e))?;
    let toml_value: toml::Value = serde_json::from_value(json)
        .map_err(|e| format!("Cannot convert to TOML: {}", e))?;
    toml::to_string_pretty(&toml_value).map_err(|e| format!("TOML serialize error: {}", e))
}

pub fn yaml_to_json(input: &str) -> Result<String, String> {
    let value: serde_yaml::Value =
        serde_yaml::from_str(input.trim()).map_err(|e| format!("Invalid YAML: {}", e))?;
    // Convert via serde_json::Value for pretty printing
    let json_value: serde_json::Value = serde_json::to_value(value)
        .map_err(|e| format!("JSON conversion error: {}", e))?;
    serde_json::to_string_pretty(&json_value).map_err(|e| format!("JSON serialize error: {}", e))
}

pub fn toml_to_json(input: &str) -> Result<String, String> {
    let value: toml::Value =
        toml::from_str(input.trim()).map_err(|e| format!("Invalid TOML: {}", e))?;
    let json_value: serde_json::Value = serde_json::to_value(value)
        .map_err(|e| format!("JSON conversion error: {}", e))?;
    serde_json::to_string_pretty(&json_value).map_err(|e| format!("JSON serialize error: {}", e))
}

// ── Hash generators ─────────────────────────────────────────────────

pub fn hash_md5(input: &str) -> Result<String, String> {
    let mut hasher = Md5::new();
    hasher.update(input.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn hash_sha1(input: &str) -> Result<String, String> {
    let mut hasher = Sha1::new();
    hasher.update(input.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn hash_sha256(input: &str) -> Result<String, String> {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

// ── Count ───────────────────────────────────────────────────────────

pub fn count(input: &str) -> Result<String, String> {
    let characters = input.len();
    let characters_no_spaces = input.chars().filter(|c| !c.is_whitespace()).count();
    let words = input.split_whitespace().count();
    let lines = if input.is_empty() { 0 } else { input.lines().count() };
    let bytes = input.as_bytes().len();

    let result = serde_json::json!({
        "characters": characters,
        "characters_no_spaces": characters_no_spaces,
        "words": words,
        "lines": lines,
        "bytes": bytes,
    });

    serde_json::to_string(&result).map_err(|e| format!("Serialize error: {}", e))
}

// ── Timestamp operations ─────────────────────────────────────────────

pub fn timestamp_iso() -> Result<String, String> {
    Ok(chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%:z").to_string())
}

pub fn timestamp_unix() -> Result<String, String> {
    Ok(chrono::Utc::now().timestamp().to_string())
}

pub fn timestamp_utc() -> Result<String, String> {
    Ok(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

pub fn unix_to_date(input: &str) -> Result<String, String> {
    let trimmed = input.trim();

    // Handle seconds or milliseconds
    let ts: i64 = trimmed
        .parse()
        .map_err(|_| format!("'{}' is not a valid Unix timestamp", trimmed))?;

    let ts_secs = if ts > 9_999_999_999 { ts / 1000 } else { ts };

    let dt = chrono::DateTime::from_timestamp(ts_secs, 0)
        .ok_or("Timestamp out of range")?;

    let local = dt.with_timezone(&chrono::Local);
    Ok(format!(
        "Local:  {}\nUTC:    {}\nUnix:   {}",
        local.format("%Y-%m-%d %H:%M:%S %Z"),
        dt.format("%Y-%m-%d %H:%M:%S UTC"),
        ts_secs,
    ))
}

pub fn date_to_unix(input: &str) -> Result<String, String> {
    let trimmed = input.trim();

    // Try ISO 8601 with timezone
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return Ok(dt.timestamp().to_string());
    }

    // Try ISO 8601 without timezone (assume local)
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S") {
        let local = naive
            .and_local_timezone(chrono::Local)
            .single()
            .ok_or("Ambiguous local time")?;
        return Ok(local.timestamp().to_string());
    }

    // Try common date formats
    let formats = [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%d",
        "%m/%d/%Y %H:%M:%S",
        "%m/%d/%Y",
        "%d %b %Y %H:%M:%S",
        "%d %b %Y",
    ];

    for fmt in &formats {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(trimmed, fmt) {
            let local = naive
                .and_local_timezone(chrono::Local)
                .single()
                .ok_or("Ambiguous local time")?;
            return Ok(local.timestamp().to_string());
        }
        // Try date-only formats (set time to midnight)
        if let Ok(date) = chrono::NaiveDate::parse_from_str(trimmed, fmt) {
            let naive = date.and_hms_opt(0, 0, 0).ok_or("Invalid date")?;
            let local = naive
                .and_local_timezone(chrono::Local)
                .single()
                .ok_or("Ambiguous local time")?;
            return Ok(local.timestamp().to_string());
        }
    }

    Err(format!("Could not parse '{}' as a date. Try formats like: 2024-01-15, 2024-01-15T10:30:00, 01/15/2024", trimmed))
}

// ── XML/YAML ────────────────────────────────────────────────────────

pub fn format_xml(input: &str) -> Result<String, String> {
    let mut reader = quick_xml::Reader::from_str(input.trim());
    let mut writer = quick_xml::Writer::new_with_indent(Vec::new(), b' ', 2);

    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(event) => writer
                .write_event(event)
                .map_err(|e| format!("XML write error: {}", e))?,
            Err(e) => return Err(format!("Invalid XML: {}", e)),
        }
    }

    String::from_utf8(writer.into_inner()).map_err(|e| format!("UTF-8 error: {}", e))
}

pub fn format_yaml(input: &str) -> Result<String, String> {
    let value: serde_yaml::Value =
        serde_yaml::from_str(input.trim()).map_err(|e| format!("Invalid YAML: {}", e))?;
    serde_yaml::to_string(&value).map_err(|e| format!("YAML format error: {}", e))
}

// ── Clipboard detection ─────────────────────────────────────────────

pub fn detect_content(input: &str) -> Vec<String> {
    let trimmed = input.trim();
    let mut suggestions = Vec::new();

    if trimmed.is_empty() {
        return suggestions;
    }

    // JSON
    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
            suggestions.push("format_json".into());
        }
    }

    // XML
    if trimmed.starts_with('<') && trimmed.ends_with('>') {
        suggestions.push("format_xml".into());
    }

    // YAML (starts with key: or --- or has multiple key: value lines)
    if trimmed.starts_with("---")
        || (trimmed.contains(": ") && !trimmed.starts_with('{') && !trimmed.starts_with('<'))
    {
        if serde_yaml::from_str::<serde_yaml::Value>(trimmed).is_ok() {
            suggestions.push("format_yaml".into());
        }
    }

    // JWT (three dot-separated base64url segments)
    {
        let parts: Vec<&str> = trimmed.split('.').collect();
        if parts.len() == 3
            && parts.iter().all(|p| {
                !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '=')
            })
        {
            suggestions.push("jwt_decode".into());
        }
    }

    // Base64 (only ASCII alphanumeric, +, /, =, and reasonable length)
    if trimmed.len() >= 4
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
        && general_purpose::STANDARD.decode(trimmed).is_ok()
        && !suggestions.contains(&"jwt_decode".into())
    {
        suggestions.push("base64_decode".into());
    }

    // URL-encoded (contains %XX patterns)
    if trimmed.contains('%')
        && trimmed.chars().any(|c| c.is_ascii_hexdigit())
    {
        let has_pct = trimmed
            .as_bytes()
            .windows(3)
            .any(|w| w[0] == b'%' && w[1..].iter().all(|b| b.is_ascii_hexdigit()));
        if has_pct {
            suggestions.push("url_decode".into());
        }
    }

    // Hex string (all hex chars, even length, at least 4)
    {
        let hex_candidate = trimmed.strip_prefix("0x").unwrap_or(trimmed);
        let clean: String = hex_candidate.chars().filter(|c| !c.is_whitespace()).collect();
        if clean.len() >= 4
            && clean.len() % 2 == 0
            && clean.chars().all(|c| c.is_ascii_hexdigit())
            && !clean.chars().all(|c| c.is_ascii_digit()) // not just numbers
        {
            suggestions.push("hex_decode".into());
        }
    }

    // HTML entities
    if trimmed.contains("&amp;")
        || trimmed.contains("&lt;")
        || trimmed.contains("&gt;")
        || trimmed.contains("&quot;")
        || trimmed.contains("&#")
    {
        suggestions.push("html_decode".into());
    }

    // Color values
    if trimmed.starts_with('#') && (trimmed.len() == 4 || trimmed.len() == 7)
        && trimmed[1..].chars().all(|c| c.is_ascii_hexdigit())
    {
        suggestions.push("color_convert".into());
    }
    if trimmed.starts_with("rgb(") || trimmed.starts_with("hsl(") {
        suggestions.push("color_convert".into());
    }

    // Number with prefix (0x, 0b, 0o)
    if (trimmed.starts_with("0x") || trimmed.starts_with("0b") || trimmed.starts_with("0o"))
        && trimmed.len() > 2
    {
        suggestions.push("number_convert".into());
    }

    // TOML (has [section] headers and key = value)
    if trimmed.contains("[") && trimmed.contains("]") && trimmed.contains(" = ") {
        if toml::from_str::<toml::Value>(trimmed).is_ok() {
            suggestions.push("toml_to_json".into());
        }
    }

    // Markdown (has # headers, **bold**, [links](url), or ```code```)
    if (trimmed.contains("# ") || trimmed.contains("**") || trimmed.contains("](") || trimmed.contains("```"))
        && !trimmed.starts_with('<')
    {
        suggestions.push("md_to_html".into());
    }

    // HTML tags (for html_to_md — if it has closing tags, it's likely HTML)
    if trimmed.contains("</") && trimmed.contains('>') {
        suggestions.push("html_to_md".into());
    }

    // Unix timestamp (10 or 13 digit number)
    if let Ok(n) = trimmed.parse::<i64>() {
        if (1_000_000_000..=9_999_999_999_999).contains(&n) {
            suggestions.push("unix_to_date".into());
        }
    }

    // Date string → suggest date_to_unix
    if chrono::DateTime::parse_from_rfc3339(trimmed).is_ok()
        || chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S").is_ok()
        || chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S").is_ok()
        || chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d").is_ok()
    {
        suggestions.push("date_to_unix".into());
    }

    suggestions
}

// ── Secret operations ───────────────────────────────────────────────

pub fn get_secret(key: &str) -> Result<String, String> {
    let entry =
        Entry::new(SERVICE_NAME, key).map_err(|e| format!("Failed to access keychain: {}", e))?;
    let password = entry
        .get_password()
        .map_err(|e| format!("Secret '{}' not found: {}", key, e))?;
    Ok(password)
}

pub fn set_secret(key: &str, value: &str) -> Result<(), String> {
    let entry =
        Entry::new(SERVICE_NAME, key).map_err(|e| format!("Failed to access keychain: {}", e))?;
    entry
        .set_password(value)
        .map_err(|e| format!("Failed to store secret: {}", e))
}

pub fn delete_secret(key: &str) -> Result<(), String> {
    let entry =
        Entry::new(SERVICE_NAME, key).map_err(|e| format!("Failed to access keychain: {}", e))?;
    entry
        .delete_credential()
        .map_err(|e| format!("Failed to delete secret: {}", e))
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── GUID ────────────────────────────────────────────────────────

    #[test]
    fn test_generate_guid() {
        let guid = generate_guid().unwrap();
        assert_eq!(guid.len(), 36); // 8-4-4-4-12
        assert_eq!(guid.chars().filter(|c| *c == '-').count(), 4);
    }

    #[test]
    fn test_generate_guid_unique() {
        let a = generate_guid().unwrap();
        let b = generate_guid().unwrap();
        assert_ne!(a, b);
    }

    // ── JSON ────────────────────────────────────────────────────────

    #[test]
    fn test_format_json_object() {
        let result = format_json(r#"{"a":1,"b":"hello"}"#).unwrap();
        assert!(result.contains("\"a\": 1"));
        assert!(result.contains("\"b\": \"hello\""));
    }

    #[test]
    fn test_format_json_array() {
        let result = format_json("[1,2,3]").unwrap();
        assert!(result.contains("[\n"));
    }

    #[test]
    fn test_format_json_invalid() {
        assert!(format_json("not json").is_err());
    }

    // ── Base64 ──────────────────────────────────────────────────────

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode("hello").unwrap(), "aGVsbG8=");
    }

    #[test]
    fn test_base64_decode() {
        assert_eq!(base64_decode("aGVsbG8=").unwrap(), "hello");
    }

    #[test]
    fn test_base64_decode_invalid() {
        assert!(base64_decode("!!!not-base64!!!").is_err());
    }

    #[test]
    fn test_base64_roundtrip() {
        let input = "The quick brown fox 🦊";
        let encoded = base64_encode(input).unwrap();
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, input);
    }

    // ── URL encoding ────────────────────────────────────────────────

    #[test]
    fn test_url_encode() {
        assert_eq!(url_encode("hello world").unwrap(), "hello%20world");
    }

    #[test]
    fn test_url_encode_special() {
        let result = url_encode("a=1&b=2").unwrap();
        assert!(result.contains("%26"));
        assert!(result.contains("%3D"));
    }

    #[test]
    fn test_url_decode() {
        assert_eq!(url_decode("hello%20world").unwrap(), "hello world");
    }

    #[test]
    fn test_url_roundtrip() {
        let input = "key=value&foo=bar baz";
        let encoded = url_encode(input).unwrap();
        let decoded = url_decode(&encoded).unwrap();
        assert_eq!(decoded, input);
    }

    // ── JWT ──────────────────────────────────────────────────────────

    #[test]
    fn test_jwt_decode() {
        // header: {"alg":"HS256","typ":"JWT"}, payload: {"sub":"1234567890","name":"John","iat":1516239022}
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4iLCJpYXQiOjE1MTYyMzkwMjJ9.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let result = jwt_decode(token).unwrap();
        assert!(result.contains("\"alg\": \"HS256\""));
        assert!(result.contains("\"name\": \"John\""));
    }

    #[test]
    fn test_jwt_decode_invalid() {
        assert!(jwt_decode("not.a.jwt.token.here").is_err());
        assert!(jwt_decode("single").is_err());
    }

    // ── Hex ─────────────────────────────────────────────────────────

    #[test]
    fn test_hex_encode() {
        assert_eq!(hex_encode("AB").unwrap(), "4142");
    }

    #[test]
    fn test_hex_decode() {
        assert_eq!(hex_decode("4142").unwrap(), "AB");
    }

    #[test]
    fn test_hex_decode_0x_prefix() {
        assert_eq!(hex_decode("0x4142").unwrap(), "AB");
    }

    #[test]
    fn test_hex_decode_odd_length() {
        assert!(hex_decode("414").is_err());
    }

    #[test]
    fn test_hex_roundtrip() {
        let input = "Hello, World!";
        let encoded = hex_encode(input).unwrap();
        let decoded = hex_decode(&encoded).unwrap();
        assert_eq!(decoded, input);
    }

    // ── HTML decode ─────────────────────────────────────────────────

    #[test]
    fn test_html_decode_basic() {
        assert_eq!(html_decode("&lt;div&gt;").unwrap(), "<div>");
    }

    #[test]
    fn test_html_decode_amp() {
        assert_eq!(html_decode("a &amp; b").unwrap(), "a & b");
    }

    #[test]
    fn test_html_decode_numeric() {
        assert_eq!(html_decode("&#65;&#66;").unwrap(), "AB");
    }

    #[test]
    fn test_html_decode_hex_numeric() {
        assert_eq!(html_decode("&#x41;&#x42;").unwrap(), "AB");
    }

    #[test]
    fn test_html_decode_emoji() {
        assert_eq!(html_decode("&#x1F600;").unwrap(), "😀");
    }

    #[test]
    fn test_html_decode_passthrough() {
        assert_eq!(html_decode("no entities here").unwrap(), "no entities here");
    }

    // ── Timestamps ──────────────────────────────────────────────────

    #[test]
    fn test_timestamp_iso() {
        let result = timestamp_iso().unwrap();
        // Should contain date separator and timezone offset
        assert!(result.contains("T"));
        assert!(result.contains("+") || result.contains("-") || result.contains("Z"));
    }

    #[test]
    fn test_timestamp_unix() {
        let result = timestamp_unix().unwrap();
        let ts: i64 = result.parse().unwrap();
        assert!(ts > 1_000_000_000); // after 2001
    }

    #[test]
    fn test_timestamp_utc() {
        let result = timestamp_utc().unwrap();
        assert!(result.ends_with("Z"));
        assert!(result.contains("T"));
    }

    #[test]
    fn test_unix_to_date_seconds() {
        let result = unix_to_date("1700000000").unwrap();
        assert!(result.contains("2023"));
        assert!(result.contains("Nov") || result.contains("11"));
    }

    #[test]
    fn test_unix_to_date_milliseconds() {
        let result = unix_to_date("1700000000000").unwrap();
        assert!(result.contains("2023"));
    }

    #[test]
    fn test_unix_to_date_invalid() {
        assert!(unix_to_date("not a number").is_err());
    }

    #[test]
    fn test_date_to_unix_iso() {
        let result = date_to_unix("2023-11-14T22:13:20+00:00").unwrap();
        assert_eq!(result, "1700000000");
    }

    #[test]
    fn test_date_to_unix_date_only() {
        let result = date_to_unix("2023-01-01").unwrap();
        let ts: i64 = result.parse().unwrap();
        assert!(ts > 1_672_000_000 && ts < 1_673_000_000);
    }

    #[test]
    fn test_date_to_unix_invalid() {
        assert!(date_to_unix("not a date").is_err());
    }

    // ── XML ─────────────────────────────────────────────────────────

    #[test]
    fn test_format_xml() {
        let result = format_xml("<root><child>text</child></root>").unwrap();
        assert!(result.contains("  <child>"));
    }

    #[test]
    fn test_format_xml_preserves_content() {
        let result = format_xml("<a><b>text</b><c/></a>").unwrap();
        assert!(result.contains("text"));
        assert!(result.contains("<c/>") || result.contains("<c />"));
    }

    // ── Count ────────────────────────────────────────────────────────

    // ── Markdown ↔ HTML ───────────────────────────────────────────────

    #[test]
    fn test_md_to_html_heading() {
        let result = markdown_to_html("# Hello").unwrap();
        assert!(result.contains("<h1>Hello</h1>"));
    }

    #[test]
    fn test_md_to_html_bold_italic() {
        let result = markdown_to_html("**bold** and *italic*").unwrap();
        assert!(result.contains("<strong>bold</strong>"));
        assert!(result.contains("<em>italic</em>"));
    }

    #[test]
    fn test_md_to_html_link() {
        let result = markdown_to_html("[click](https://example.com)").unwrap();
        assert!(result.contains("<a href=\"https://example.com\">click</a>"));
    }

    #[test]
    fn test_md_to_html_list() {
        let result = markdown_to_html("- one\n- two").unwrap();
        assert!(result.contains("<li>one</li>"));
        assert!(result.contains("<li>two</li>"));
    }

    #[test]
    fn test_html_to_md_heading() {
        let result = html_to_markdown("<h1>Hello</h1>").unwrap();
        assert!(result.starts_with("# Hello"));
    }

    #[test]
    fn test_html_to_md_bold() {
        let result = html_to_markdown("<strong>bold</strong>").unwrap();
        assert!(result.contains("**bold**"));
    }

    #[test]
    fn test_html_to_md_paragraph() {
        let result = html_to_markdown("<p>First</p><p>Second</p>").unwrap();
        assert!(result.contains("First"));
        assert!(result.contains("Second"));
    }

    #[test]
    fn test_html_to_md_list() {
        let result = html_to_markdown("<ul><li>one</li><li>two</li></ul>").unwrap();
        assert!(result.contains("- one"));
        assert!(result.contains("- two"));
    }

    #[test]
    fn test_html_to_md_code() {
        let result = html_to_markdown("<code>hello</code>").unwrap();
        assert!(result.contains("`hello`"));
    }

    #[test]
    fn test_detect_markdown() {
        let s = detect_content("# Title\n\nSome **bold** text");
        assert!(s.contains(&"md_to_html".to_string()));
    }

    #[test]
    fn test_detect_html_for_md() {
        let s = detect_content("<h1>Title</h1><p>text</p>");
        assert!(s.contains(&"html_to_md".to_string()));
    }

    // ── Lorem ipsum ──────────────────────────────────────────────────

    #[test]
    fn test_lorem_words() {
        let result = lorem_ipsum("10 words").unwrap();
        let words: Vec<&str> = result.trim_end_matches('.').split_whitespace().collect();
        assert_eq!(words.len(), 10);
    }

    #[test]
    fn test_lorem_sentences() {
        let result = lorem_ipsum("3 sentences").unwrap();
        let sentences: Vec<&str> = result.split(". ").collect();
        assert!(sentences.len() >= 3);
    }

    #[test]
    fn test_lorem_paragraphs() {
        let result = lorem_ipsum("2 paragraphs").unwrap();
        let paragraphs: Vec<&str> = result.split("\n\n").collect();
        assert_eq!(paragraphs.len(), 2);
    }

    #[test]
    fn test_lorem_just_number() {
        let result = lorem_ipsum("5").unwrap();
        let words: Vec<&str> = result.trim_end_matches('.').split_whitespace().collect();
        assert_eq!(words.len(), 5);
    }

    #[test]
    fn test_lorem_capitalized() {
        let result = lorem_ipsum("3 words").unwrap();
        assert!(result.chars().next().unwrap().is_uppercase());
    }

    // ── Dice roller ─────────────────────────────────────────────────

    fn parse_roll(json: &str) -> (i64, String) {
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        (v["total"].as_i64().unwrap(), v["rolls"].as_str().unwrap().to_string())
    }

    #[test]
    fn test_roll_d20() {
        let (total, rolls) = parse_roll(&roll_dice("1d20").unwrap());
        assert!((1..=20).contains(&total));
        assert!(rolls.starts_with('[') && rolls.ends_with(']'));
    }

    #[test]
    fn test_roll_leading_n_optional() {
        let (total, _) = parse_roll(&roll_dice("d6").unwrap());
        assert!((1..=6).contains(&total));
    }

    #[test]
    fn test_roll_with_positive_modifier() {
        let (total, rolls) = parse_roll(&roll_dice("3d6+2").unwrap());
        assert!((5..=20).contains(&total)); // 3..18 + 2
        assert!(rolls.contains("+ 2"));
    }

    #[test]
    fn test_roll_with_negative_modifier() {
        let (total, rolls) = parse_roll(&roll_dice("2d8-1").unwrap());
        assert!((1..=15).contains(&total)); // 2..16 - 1
        assert!(rolls.contains("- 1"));
    }

    #[test]
    fn test_roll_case_insensitive() {
        let (total, _) = parse_roll(&roll_dice("1D20").unwrap());
        assert!((1..=20).contains(&total));
    }

    #[test]
    fn test_roll_whitespace_in_modifier() {
        let (total, _) = parse_roll(&roll_dice("2d6 + 3").unwrap());
        assert!((5..=15).contains(&total));
    }

    #[test]
    fn test_roll_invalid() {
        assert!(roll_dice("").is_err());
        assert!(roll_dice("hello").is_err());
        assert!(roll_dice("1d").is_err());
        assert!(roll_dice("d").is_err());
        assert!(roll_dice("0d6").is_err());
        assert!(roll_dice("1d1").is_err());
        assert!(roll_dice("1d20+").is_err());
    }

    #[test]
    fn test_roll_caps_limits() {
        assert!(roll_dice("101d6").is_err());
        assert!(roll_dice("1d1001").is_err());
    }

    // ── Regex extract ───────────────────────────────────────────────

    #[test]
    fn test_regex_extract_emails() {
        let input = "Contact us at alice@example.com or bob@test.org";
        let result = regex_extract(input, r"[\w.-]+@[\w.-]+").unwrap();
        assert!(result.contains("alice@example.com"));
        assert!(result.contains("bob@test.org"));
    }

    #[test]
    fn test_regex_extract_numbers() {
        let result = regex_extract("abc 123 def 456 ghi", r"\d+").unwrap();
        assert!(result.contains("123"));
        assert!(result.contains("456"));
    }

    #[test]
    fn test_regex_extract_capture_groups() {
        let input = "name=alice, name=bob";
        let result = regex_extract(input, r"name=(\w+)").unwrap();
        assert!(result.contains("alice"));
        assert!(result.contains("bob"));
        // Should NOT contain "name=" since we're extracting capture group 1
        assert!(!result.contains("name="));
    }

    #[test]
    fn test_regex_extract_no_match() {
        assert!(regex_extract("hello world", r"\d+").is_err());
    }

    #[test]
    fn test_regex_extract_invalid_pattern() {
        assert!(regex_extract("test", r"[invalid").is_err());
    }

    // ── Number base converter ───────────────────────────────────────

    #[test]
    fn test_number_convert_decimal() {
        let result = number_convert("255").unwrap();
        assert!(result.contains("0xff"));
        assert!(result.contains("0b11111111"));
        assert!(result.contains("0o377"));
    }

    #[test]
    fn test_number_convert_hex() {
        let result = number_convert("0xff").unwrap();
        assert!(result.contains("Decimal:  255"));
    }

    #[test]
    fn test_number_convert_binary() {
        let result = number_convert("0b1010").unwrap();
        assert!(result.contains("Decimal:  10"));
    }

    #[test]
    fn test_number_convert_octal() {
        let result = number_convert("0o77").unwrap();
        assert!(result.contains("Decimal:  63"));
    }

    // ── Color converter ─────────────────────────────────────────────

    #[test]
    fn test_color_hex() {
        let result = color_convert("#ff0000").unwrap();
        assert!(result.contains("rgb(255, 0, 0)"));
        assert!(result.contains("hsl(0,"));
    }

    #[test]
    fn test_color_hex_short() {
        let result = color_convert("#fff").unwrap();
        assert!(result.contains("rgb(255, 255, 255)"));
    }

    #[test]
    fn test_color_rgb() {
        let result = color_convert("rgb(0, 128, 255)").unwrap();
        assert!(result.contains("#0080ff"));
    }

    #[test]
    fn test_color_hsl() {
        let result = color_convert("hsl(0, 100%, 50%)").unwrap();
        assert!(result.contains("rgb(255, 0, 0)"));
    }

    #[test]
    fn test_color_invalid() {
        assert!(color_convert("not a color").is_err());
    }

    // ── Config format converters ────────────────────────────────────

    #[test]
    fn test_json_to_yaml() {
        let result = json_to_yaml(r#"{"name": "test", "value": 42}"#).unwrap();
        assert!(result.contains("name:"));
        assert!(result.contains("42"));
    }

    #[test]
    fn test_yaml_to_json() {
        let result = yaml_to_json("name: test\nvalue: 42").unwrap();
        assert!(result.contains("\"name\": \"test\""));
        assert!(result.contains("\"value\": 42"));
    }

    #[test]
    fn test_toml_to_json() {
        let result = toml_to_json("[package]\nname = \"test\"\nversion = \"1.0\"").unwrap();
        assert!(result.contains("\"name\": \"test\""));
    }

    #[test]
    fn test_detect_color_hex() {
        let s = detect_content("#ff5500");
        assert!(s.contains(&"color_convert".to_string()));
    }

    #[test]
    fn test_detect_color_rgb() {
        let s = detect_content("rgb(255, 0, 0)");
        assert!(s.contains(&"color_convert".to_string()));
    }

    #[test]
    fn test_detect_number_hex() {
        let s = detect_content("0xff");
        assert!(s.contains(&"number_convert".to_string()));
    }

    // ── Hashes ───────────────────────────────────────────────────────

    #[test]
    fn test_hash_md5() {
        assert_eq!(hash_md5("hello").unwrap(), "5d41402abc4b2a76b9719d911017c592");
    }

    #[test]
    fn test_hash_sha1() {
        assert_eq!(hash_sha1("hello").unwrap(), "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d");
    }

    #[test]
    fn test_hash_sha256() {
        assert_eq!(hash_sha256("hello").unwrap(), "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    }

    #[test]
    fn test_hash_empty() {
        // MD5 of empty string is a known value
        assert_eq!(hash_md5("").unwrap(), "d41d8cd98f00b204e9800998ecf8427e");
    }

    // ── Count ────────────────────────────────────────────────────────

    #[test]
    fn test_count_basic() {
        let result = count("hello world").unwrap();
        let data: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(data["characters"], 11);
        assert_eq!(data["words"], 2);
        assert_eq!(data["lines"], 1);
    }

    #[test]
    fn test_count_multiline() {
        let result = count("line one\nline two\nline three").unwrap();
        let data: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(data["lines"], 3);
        assert_eq!(data["words"], 6);
    }

    #[test]
    fn test_count_no_spaces() {
        let result = count("a b c").unwrap();
        let data: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(data["characters"], 5);
        assert_eq!(data["characters_no_spaces"], 3);
    }

    #[test]
    fn test_count_empty() {
        let result = count("").unwrap();
        let data: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(data["characters"], 0);
        assert_eq!(data["words"], 0);
        assert_eq!(data["lines"], 0);
    }

    // ── YAML ────────────────────────────────────────────────────────

    #[test]
    fn test_format_yaml() {
        let result = format_yaml("name: John\nage: 30").unwrap();
        assert!(result.contains("name:"));
        assert!(result.contains("age:"));
    }

    #[test]
    fn test_format_yaml_nested() {
        let result = format_yaml("{a: {b: 1, c: 2}}").unwrap();
        assert!(result.contains("b:"));
    }

    // ── Clipboard detection ─────────────────────────────────────────

    #[test]
    fn test_detect_json() {
        let s = detect_content(r#"{"key": "value"}"#);
        assert!(s.contains(&"format_json".to_string()));
    }

    #[test]
    fn test_detect_json_array() {
        let s = detect_content("[1, 2, 3]");
        assert!(s.contains(&"format_json".to_string()));
    }

    #[test]
    fn test_detect_xml() {
        let s = detect_content("<root><child/></root>");
        assert!(s.contains(&"format_xml".to_string()));
    }

    #[test]
    fn test_detect_jwt() {
        let token = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.signature";
        let s = detect_content(token);
        assert!(s.contains(&"jwt_decode".to_string()));
    }

    #[test]
    fn test_detect_base64() {
        let s = detect_content("aGVsbG8gd29ybGQ=");
        assert!(s.contains(&"base64_decode".to_string()));
    }

    #[test]
    fn test_detect_url_encoded() {
        let s = detect_content("hello%20world%21");
        assert!(s.contains(&"url_decode".to_string()));
    }

    #[test]
    fn test_detect_hex() {
        let s = detect_content("48656c6c6f");
        assert!(s.contains(&"hex_decode".to_string()));
    }

    #[test]
    fn test_detect_html_entities() {
        let s = detect_content("&lt;div&gt;hello&lt;/div&gt;");
        assert!(s.contains(&"html_decode".to_string()));
    }

    #[test]
    fn test_detect_unix_timestamp() {
        let s = detect_content("1700000000");
        assert!(s.contains(&"unix_to_date".to_string()));
    }

    #[test]
    fn test_detect_unix_timestamp_millis() {
        let s = detect_content("1700000000000");
        assert!(s.contains(&"unix_to_date".to_string()));
    }

    #[test]
    fn test_detect_date_iso() {
        let s = detect_content("2023-11-14T22:13:20+00:00");
        assert!(s.contains(&"date_to_unix".to_string()));
    }

    #[test]
    fn test_detect_date_simple() {
        let s = detect_content("2023-01-15");
        assert!(s.contains(&"date_to_unix".to_string()));
    }

    #[test]
    fn test_detect_empty() {
        assert!(detect_content("").is_empty());
    }

    #[test]
    fn test_detect_plain_text_no_false_positives() {
        let s = detect_content("just some regular text");
        assert!(!s.contains(&"format_json".to_string()));
        assert!(!s.contains(&"jwt_decode".to_string()));
        assert!(!s.contains(&"base64_decode".to_string()));
    }
}

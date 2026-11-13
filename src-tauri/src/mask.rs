use aho_corasick::{AhoCorasick, Input, MatchKind};
use base64::{engine::general_purpose, Engine};

const MIN_DERIVED_LEN: usize = 6;

pub struct Masker {
    ac: AhoCorasick,
    patterns: Vec<Vec<u8>>,
    labels: Vec<Vec<u8>>,
    max_len: usize,
    buf: Vec<u8>,
}

// A value embedded in a larger base64 payload (e.g. "user:token" in Basic auth) encodes
// differently per byte offset mod 3; keep only the characters fixed by the value itself.
fn base64_variants(value: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    for engine in [&general_purpose::STANDARD_NO_PAD, &general_purpose::URL_SAFE_NO_PAD] {
        for (offset, skip) in [(0usize, 0usize), (1, 2), (2, 3)] {
            let mut data = vec![0u8; offset];
            data.extend_from_slice(value);
            let encoded = engine.encode(&data);
            let full = data.len() * 8 / 6;
            if full > skip {
                out.push(encoded[skip..full].to_string());
            }
        }
    }
    out
}

fn variants(value: &str) -> Vec<Vec<u8>> {
    let mut out = vec![value.to_string(), value.trim().to_string()];
    let mut derived = vec![
        urlencoding::encode(value).into_owned(),
        value.bytes().map(|b| format!("{:02x}", b)).collect(),
        value.bytes().map(|b| format!("{:02X}", b)).collect(),
    ];
    derived.extend(base64_variants(value.as_bytes()));
    out.extend(derived.into_iter().filter(|d| d.len() >= MIN_DERIVED_LEN));
    out.retain(|s| !s.is_empty());
    out.sort();
    out.dedup();
    out.into_iter().map(String::into_bytes).collect()
}

impl Masker {
    pub fn new(secrets: &[(String, String)]) -> Result<Self, String> {
        let mut patterns = Vec::new();
        let mut labels = Vec::new();
        for (name, value) in secrets {
            for p in variants(value) {
                patterns.push(p);
                labels.push(format!("<masked:{}>", name).into_bytes());
            }
        }
        let ac = AhoCorasick::builder()
            .match_kind(MatchKind::LeftmostLongest)
            .build(&patterns)
            .map_err(|e| format!("Failed to build output mask: {}", e))?;
        let max_len = patterns.iter().map(Vec::len).max().unwrap_or(0);
        Ok(Self { ac, patterns, labels, max_len, buf: Vec::new() })
    }

    pub fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        self.buf.extend_from_slice(chunk);
        self.drain(false)
    }

    pub fn finish(&mut self) -> Vec<u8> {
        self.drain(true)
    }

    // First position whose remaining bytes could still grow into a longer match.
    fn first_open(&self, from: usize) -> usize {
        let len = self.buf.len();
        let start = from.max(len.saturating_sub(self.max_len.saturating_sub(1)));
        (start..len)
            .find(|&s| {
                let tail = &self.buf[s..];
                self.patterns.iter().any(|p| p.len() > tail.len() && p.starts_with(tail))
            })
            .unwrap_or(len)
    }

    fn drain(&mut self, eof: bool) -> Vec<u8> {
        let len = self.buf.len();
        let mut out = Vec::with_capacity(len);
        let mut last = 0;
        loop {
            let hold = if eof { len } else { self.first_open(last) };
            match self.ac.find(Input::new(&self.buf).span(last..len)) {
                Some(m) if m.start() < hold => {
                    out.extend_from_slice(&self.buf[last..m.start()]);
                    out.extend_from_slice(&self.labels[m.pattern().as_usize()]);
                    last = m.end();
                }
                _ => {
                    out.extend_from_slice(&self.buf[last..hold]);
                    self.buf.drain(..hold);
                    return out;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn masker(secrets: &[(&str, &str)]) -> Masker {
        let owned: Vec<(String, String)> =
            secrets.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect();
        Masker::new(&owned).unwrap()
    }

    fn run(secrets: &[(&str, &str)], chunks: &[&[u8]]) -> String {
        let mut m = masker(secrets);
        let mut out = Vec::new();
        for c in chunks {
            out.extend(m.push(c));
        }
        out.extend(m.finish());
        String::from_utf8(out).unwrap()
    }

    const TOKEN: &str = "ghp_s3cr3tT0kenValue";

    #[test]
    fn masks_raw_value() {
        let input = format!("token={};\n", TOKEN);
        assert_eq!(run(&[("gh", TOKEN)], &[input.as_bytes()]), "token=<masked:gh>;\n");
    }

    #[test]
    fn masks_value_split_one_byte_per_chunk() {
        let input = format!("a {} b {} c", TOKEN, TOKEN);
        let chunks: Vec<&[u8]> = input.as_bytes().chunks(1).collect();
        assert_eq!(run(&[("gh", TOKEN)], &chunks), "a <masked:gh> b <masked:gh> c");
    }

    #[test]
    fn never_emits_a_prefix_that_may_become_a_secret() {
        let mut m = masker(&[("gh", TOKEN)]);
        let out = String::from_utf8(m.push(b"Bearer ghp_s3cr")).unwrap();
        assert_eq!(out, "Bearer ");
        let out = String::from_utf8(m.push(b"3tT0kenValue done")).unwrap();
        assert_eq!(out, "<masked:gh> done");
    }

    #[test]
    fn flushes_unrelated_output_immediately() {
        let mut m = masker(&[("gh", TOKEN)]);
        assert_eq!(m.push(b"building...\n"), b"building...\n");
    }

    #[test]
    fn releases_an_incomplete_prefix_at_eof() {
        assert_eq!(run(&[("gh", TOKEN)], &[b"ends with ghp_s3"]), "ends with ghp_s3");
    }

    #[test]
    fn masks_value_inside_basic_auth_header() {
        for user in ["u", "us", "user"] {
            let encoded = general_purpose::STANDARD.encode(format!("{}:{}", user, TOKEN));
            let out = run(&[("gh", TOKEN)], &[format!("Authorization: Basic {}", encoded).as_bytes()]);
            let tail = out.rsplit("<masked:gh>").next().unwrap();
            assert!(out.contains("<masked:gh>") && tail.len() <= 3, "user={user}: {out}");
        }
    }

    #[test]
    fn masks_hex_in_either_case() {
        let lower: String = TOKEN.bytes().map(|b| format!("{:02x}", b)).collect();
        for hex in [lower.clone(), lower.to_uppercase()] {
            assert_eq!(run(&[("gh", TOKEN)], &[format!("x={hex}").as_bytes()]), "x=<masked:gh>");
        }
    }

    #[test]
    fn masks_url_encoded_value() {
        let secret = "p@ss/w0rd&more=yes";
        let input = format!("?pw={}", urlencoding::encode(secret));
        assert_eq!(run(&[("pw", secret)], &[input.as_bytes()]), "?pw=<masked:pw>");
    }

    #[test]
    fn prefers_the_longer_secret_across_a_chunk_boundary() {
        let out = run(&[("short", "abcdefgh"), ("long", "abcdefgh12345")], &[b"x abcdefgh", b"12345 y"]);
        assert_eq!(out, "x <masked:long> y");
    }

    #[test]
    fn passes_everything_through_without_secrets() {
        let bytes: Vec<u8> = (0..=255u8).collect();
        let mut m = Masker::new(&[]).unwrap();
        let mut out = m.push(&bytes);
        out.extend(m.finish());
        assert_eq!(out, bytes);
    }
}

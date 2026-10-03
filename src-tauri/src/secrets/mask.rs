//! Match known values before returning any child output, including line wraps.
use base64::Engine;
use zeroize::Zeroizing;

pub(crate) fn representations(value: &str) -> Vec<Zeroizing<String>> {
    let mut result = vec![Zeroizing::new(value.to_owned())];
    for engine in [
        &base64::engine::general_purpose::STANDARD,
        &base64::engine::general_purpose::STANDARD_NO_PAD,
        &base64::engine::general_purpose::URL_SAFE,
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ] {
        result.push(Zeroizing::new(engine.encode(value)));
    }
    let hex = Zeroizing::new(
        value
            .bytes()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
    );
    result.push(Zeroizing::new(hex.to_uppercase()));
    result.push(hex);
    for lower in [false, true] {
        for plus in [false, true] {
            result.push(Zeroizing::new(
                value
                    .bytes()
                    .map(|b| {
                        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                            (b as char).to_string()
                        } else if plus && b == b' ' {
                            "+".into()
                        } else if lower {
                            format!("%{b:02x}")
                        } else {
                            format!("%{b:02X}")
                        }
                    })
                    .collect(),
            ));
        }
    }
    result.sort_by_key(|s| std::cmp::Reverse(s.len()));
    result.dedup();
    result
}

pub(crate) fn mask(text: &str, needles: &[Zeroizing<String>]) -> String {
    mask_bytes(text.as_bytes(), needles)
}

pub(crate) fn mask_bytes(bytes: &[u8], needles: &[Zeroizing<String>]) -> String {
    let mut marked = vec![false; bytes.len()];
    let (flat, starts, ends) = flatten(bytes);
    let (decoded, decoded_starts, decoded_ends) = url_decode(&flat, &starts, &ends);
    for needle in needles {
        if needle.is_empty() {
            continue;
        }
        for (start, span) in bytes.windows(needle.len()).enumerate() {
            if span == needle.as_bytes() {
                marked[start..start + needle.len()].fill(true);
            }
        }
        let (clean, _, _) = flatten(needle.as_bytes());
        if clean.is_empty() {
            continue;
        }
        mark_matches(&flat, &starts, &ends, &clean, &mut marked);
        mark_matches(
            &decoded,
            &decoded_starts,
            &decoded_ends,
            &clean,
            &mut marked,
        );
    }
    let mut result = Zeroizing::new(Vec::with_capacity(bytes.len()));
    let mut redacted = false;
    for (i, byte) in bytes.iter().enumerate() {
        if marked[i] {
            if !redacted {
                result.extend_from_slice(b"[REDACTED]");
                redacted = true;
            }
        } else {
            result.push(*byte);
            redacted = false;
        }
    }
    // Decode only after masking: a wrap between UTF-8 bytes must not turn the
    // secret into replacement characters and defeat exact-byte matching.
    crate::redaction::redact_secrets(&String::from_utf8_lossy(&result))
}

fn mark_matches(
    haystack: &[u8],
    starts: &[usize],
    ends: &[usize],
    needle: &[u8],
    marked: &mut [bool],
) {
    let hex = needle.len() >= 2 && needle.iter().all(u8::is_ascii_hexdigit);
    for (i, span) in haystack.windows(needle.len()).enumerate() {
        if span == needle || (hex && span.eq_ignore_ascii_case(needle)) {
            marked[starts[i]..ends[i + needle.len() - 1]].fill(true);
        }
    }
}

fn flatten(bytes: &[u8]) -> (Zeroizing<Vec<u8>>, Vec<usize>, Vec<usize>) {
    let mut flat = Zeroizing::new(Vec::with_capacity(bytes.len()));
    let mut starts = Vec::with_capacity(bytes.len());
    let mut ends = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if matches!(bytes[i], b'\r' | b'\n') {
            i += 1;
            continue;
        }
        if bytes[i] == 0x1b && i + 1 < bytes.len() {
            if bytes[i + 1] == b'[' {
                let mut j = i + 2;
                while j < bytes.len() && !(0x40..=0x7e).contains(&bytes[j]) {
                    j += 1;
                }
                if j < bytes.len() {
                    i = j + 1;
                    continue;
                }
            } else if bytes[i + 1] == b']' {
                let mut j = i + 2;
                while j < bytes.len()
                    && bytes[j] != 7
                    && !(bytes[j] == 0x1b && bytes.get(j + 1) == Some(&b'\\'))
                {
                    j += 1;
                }
                if j < bytes.len() {
                    i = j + if bytes[j] == 7 { 1 } else { 2 };
                    continue;
                }
            }
        }
        flat.push(bytes[i]);
        starts.push(i);
        ends.push(i + 1);
        i += 1;
    }
    (flat, starts, ends)
}

fn url_decode(
    bytes: &[u8],
    starts: &[usize],
    ends: &[usize],
) -> (Zeroizing<Vec<u8>>, Vec<usize>, Vec<usize>) {
    let mut decoded = Zeroizing::new(Vec::with_capacity(bytes.len()));
    let mut ds = Vec::new();
    let mut de = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(a), Some(b)) = (digit(bytes[i + 1]), digit(bytes[i + 2]))
        {
            decoded.push(a * 16 + b);
            ds.push(starts[i]);
            de.push(ends[i + 2]);
            i += 3;
        } else {
            decoded.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
            ds.push(starts[i]);
            de.push(ends[i]);
            i += 1;
        }
    }
    (decoded, ds, de)
}
fn digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

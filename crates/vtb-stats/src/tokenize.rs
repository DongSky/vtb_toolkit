//! Naive tokenizer for danmaku word-frequency statistics.
//!
//! - Runs of ASCII alphanumerics become one lowercased token.
//! - Runs of CJK characters (Han + kana) are split into adjacent 2-char
//!   bigrams.
//! - Single-character tokens and pure-numeric tokens are dropped.

fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs
        | '\u{3400}'..='\u{4DBF}' // CJK Extension A
        | '\u{F900}'..='\u{FAFF}' // CJK Compatibility Ideographs
        | '\u{3040}'..='\u{309F}' // Hiragana
        | '\u{30A0}'..='\u{30FF}' // Katakana
    )
}

fn flush_ascii(buf: &mut String, out: &mut Vec<String>) {
    if buf.len() >= 2 && !buf.bytes().all(|b| b.is_ascii_digit()) {
        out.push(buf.to_ascii_lowercase());
    }
    buf.clear();
}

fn flush_cjk(buf: &mut Vec<char>, out: &mut Vec<String>) {
    for pair in buf.windows(2) {
        out.push(pair.iter().collect());
    }
    buf.clear();
}

/// Tokenize `text` into words for frequency counting.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut ascii = String::new();
    let mut cjk: Vec<char> = Vec::new();

    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            flush_cjk(&mut cjk, &mut out);
            ascii.push(c);
        } else if is_cjk(c) {
            flush_ascii(&mut ascii, &mut out);
            cjk.push(c);
        } else {
            flush_ascii(&mut ascii, &mut out);
            flush_cjk(&mut cjk, &mut out);
        }
    }
    flush_ascii(&mut ascii, &mut out);
    flush_cjk(&mut cjk, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::tokenize;

    #[test]
    fn ascii_words_lowercased_and_filtered() {
        assert_eq!(tokenize("Hello, World! a 123 ok42"), vec!["hello", "world", "ok42"]);
    }

    #[test]
    fn cjk_bigrams() {
        assert_eq!(tokenize("你好世界"), vec!["你好", "好世", "世界"]);
        // Single CJK char produces no token.
        assert!(tokenize("草").is_empty());
    }

    #[test]
    fn mixed_and_punctuation_splits_runs() {
        assert_eq!(tokenize("说\"你好,世界\"abc"), vec!["你好", "世界", "abc"]);
    }
}

//! Danmaku (chat) translation helpers: cheap script-based language
//! heuristics so we only spend LLM calls on chat lines that actually need
//! translating (blivechat-style 弹幕自动翻译 for overseas viewers).

/// A very cheap script-based language guess.
///
/// Returns `Some("ja")` when kana is present, `Some("ko")` for hangul,
/// `Some("zh")` for Han-only text, `Some("en")` for pure-ASCII text with
/// letters. `None` = can't tell (mixed/symbolic).
pub fn detect_script(text: &str) -> Option<&'static str> {
    let mut has_kana = false;
    let mut has_han = false;
    let mut has_hangul = false;
    let mut has_ascii_alpha = false;
    for c in text.chars() {
        match c as u32 {
            0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9D => has_kana = true,
            0x4E00..=0x9FFF | 0x3400..=0x4DBF => has_han = true,
            0xAC00..=0xD7AF | 0x1100..=0x11FF => has_hangul = true,
            _ if c.is_ascii_alphabetic() => has_ascii_alpha = true,
            _ => {}
        }
    }
    if has_kana {
        Some("ja")
    } else if has_hangul {
        Some("ko")
    } else if has_han {
        Some("zh")
    } else if has_ascii_alpha && text.is_ascii() {
        Some("en")
    } else {
        None
    }
}

/// Whether a danmaku line is worth an LLM translation call to
/// `target_lang`. Filters out noise (too short, digits/symbols/emoji
/// only) and lines that already look like the target language.
pub fn should_translate_danmaku(text: &str, target_lang: &str) -> bool {
    let t = text.trim();
    if t.chars().count() < 2 {
        return false;
    }
    // Pure numbers / punctuation / emoji ("233333", "666", "!!!", "🎉🎉")
    // carry no translatable content.
    if !t.chars().any(|c| c.is_alphabetic()) {
        return false;
    }
    match detect_script(t) {
        Some(detected) => !target_lang
            .to_ascii_lowercase()
            .starts_with(detected),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_scripts() {
        assert_eq!(detect_script("こんにちは"), Some("ja"));
        assert_eq!(detect_script("草www"), Some("zh")); // han beats ascii
        assert_eq!(detect_script("かわいい主播"), Some("ja")); // kana wins
        assert_eq!(detect_script("안녕하세요"), Some("ko"));
        assert_eq!(detect_script("主播加油"), Some("zh"));
        assert_eq!(detect_script("hello world"), Some("en"));
        assert_eq!(detect_script("!!!???"), None);
    }

    #[test]
    fn skips_noise() {
        for t in ["6", "233333", "666666", "！！！", "🎉🎉🎉", " ", ""] {
            assert!(!should_translate_danmaku(t, "ja"), "{t:?} should skip");
        }
    }

    #[test]
    fn skips_lines_already_in_target() {
        assert!(!should_translate_danmaku("かわいい！", "ja"));
        assert!(!should_translate_danmaku("主播好可爱", "zh"));
        assert!(!should_translate_danmaku("nice stream", "en"));
        // zh-CN style target codes still match the zh guess.
        assert!(!should_translate_danmaku("主播好可爱", "zh-CN"));
    }

    #[test]
    fn translates_cross_language() {
        assert!(should_translate_danmaku("主播好可爱", "ja"));
        assert!(should_translate_danmaku("かわいい！", "zh"));
        assert!(should_translate_danmaku("let's gooo", "zh"));
        // Unknown script defaults to translate.
        assert!(should_translate_danmaku("Привет из России", "zh"));
    }
}

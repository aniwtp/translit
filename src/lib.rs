use deunicode::deunicode;
use lindera::dictionary::load_dictionary;
use lindera::mode::Mode;
use lindera::segmenter::Segmenter;
use lindera::tokenizer::Tokenizer;
use pinyin::ToPinyin;
use std::sync::LazyLock;

static JAPANESE_TOKENIZER: LazyLock<Tokenizer> = LazyLock::new(|| {
    // В Lindera 4+ встроенные словари запрашиваются через схему embedded://
    let dictionary =
        load_dictionary("embedded://ipadic").expect("Failed to load embedded IPADIC dictionary");

    let segmenter = Segmenter::new(Mode::Normal, dictionary, None);
    Tokenizer::new(segmenter)
});

pub fn normalize_for_search(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let converted = if contains_japanese(trimmed) {
        japanese_to_romaji(trimmed)
    } else if contains_chinese_only(trimmed) {
        chinese_to_pinyin(trimmed)
    } else {
        trimmed.to_string()
    };

    let ascii = deunicode(&converted);
    clean_ascii(&ascii)
}

/// Транслитерация + slug с обрезанием по границе слова.
///
/// Результат — ASCII, в нижнем регистре, слова через `-`, длина <= `max_len`.
/// Длинное слово без разделителей обрезается жёстко.
pub fn slugify(input: &str, max_len: usize) -> String {
    let normalized = normalize_for_search(input);
    let slug = normalized.replace(' ', "-");
    truncate_slug(&slug, max_len)
}

fn truncate_slug(slug: &str, max_len: usize) -> String {
    if slug.len() <= max_len {
        return slug.to_owned();
    }
    // normalize_for_search гарантирует ASCII, поэтому срез по байтам безопасен
    match slug[..max_len].rfind('-') {
        Some(idx) => slug[..idx].to_owned(),
        None => slug[..max_len].to_owned(),
    }
}

fn japanese_to_romaji(text: &str) -> String {
    let mut tokens = match JAPANESE_TOKENIZER.tokenize(text) {
        Ok(t) => t,
        Err(_) => return text.to_string(),
    };

    let mut result = String::new();

    for token in tokens.iter_mut() {
        let reading_opt = {
            let details = token.details();
            details.get(7).map(|s| s.to_string())
        };

        let reading = match reading_opt {
            Some(r) => r,
            None => token.surface.to_string(),
        };

        let romaji = deunicode(&reading);
        result.push_str(&romaji);
        result.push(' ');
    }

    result
}

fn chinese_to_pinyin(text: &str) -> String {
    text.chars()
        .map(|c| if let Some(p) = c.to_pinyin() { p.plain().to_string() } else { c.to_string() })
        .collect::<Vec<_>>()
        .join(" ")
}

fn clean_ascii(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut last_was_space = true;

    for c in input.chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c.to_ascii_lowercase());
            last_was_space = false;
        } else if !last_was_space {
            result.push(' ');
            last_was_space = true;
        }
    }

    if last_was_space && !result.is_empty() {
        result.pop();
    }

    result
}

#[inline]
fn contains_japanese(s: &str) -> bool {
    s.chars().any(|c| {
        matches!(
            c,
            '\u{3040}'..='\u{309F}' | // Hiragana
            '\u{30A0}'..='\u{30FF}'   // Katakana
        )
    })
}
#[inline]
fn contains_chinese_only(s: &str) -> bool {
    s.chars().any(|c| matches!(c, '\u{4E00}'..='\u{9FFF}'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cyrillic_transliteration() {
        assert_eq!(normalize_for_search("Привет Мир"), "privet mir");
    }

    #[test]
    fn japanese_to_romaji() {
        // дефолтный маппинг deunicode для каны (シ→si, ジ→zi)
        assert_eq!(normalize_for_search("進撃の巨人"), "singeki no kiyozin");
    }

    #[test]
    fn chinese_to_pinyin() {
        assert_eq!(normalize_for_search("斗罗大陆"), "dou luo da lu");
    }

    #[test]
    fn slug_basic() {
        assert_eq!(slugify("Привет Мир", 50), "privet-mir");
        assert_eq!(slugify("Hello World! This is a test... 123?", 50), "hello-world-this-is-a-test-123");
    }

    #[test]
    fn slug_truncates_at_word_boundary() {
        assert_eq!(slugify("Hello World! This is a test... 123?", 20), "hello-world-this-is");
    }

    #[test]
    fn slug_hard_truncates_long_word() {
        assert_eq!(slugify("Супердлинноеназваниекоманды", 10), "superdlinn");
    }

    #[test]
    fn slug_never_exceeds_max_len() {
        let long = "Очень длинное название команды, которое должно быть обрезано по границе слова";
        let slug = slugify(long, 25);
        assert!(slug.len() <= 25);
        assert!(!slug.ends_with('-'));
        assert!(!slug.contains("--"));
    }

    #[test]
    fn slug_empty_and_zero() {
        assert_eq!(slugify("", 50), "");
        assert_eq!(slugify("Привет Мир", 0), "");
    }

    #[test]
    fn slug_matches_old_behavior() {
        // Старое поведение: rslug::Slugifier::new().truncate(50).slugify(name)
        assert_eq!(slugify("Атака Титанов", 50), "ataka-titanov");
        assert_eq!(slugify("Небо 9", 50), "nebo-9");
    }
}

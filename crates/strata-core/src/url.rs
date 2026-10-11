//! URL utilities — нормализация, поиск, проверка схем.

use crate::error::{StrataError, StrataResult};
use url::Url;

/// Превратить пользовательский ввод в валидный URL.
/// Если строка похожа на URL (со схемой) — парсим как есть.
/// Иначе — оборачиваем в DuckDuckGo поиск.
pub fn resolve_input(input: &str) -> StrataResult<Url> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(StrataError::InvalidUrl("empty input".into()));
    }

    // Похоже на URL со схемой?
    if let Ok(u) = Url::parse(trimmed) {
        match u.scheme() {
            "http" | "https" | "file" | "about" => return Ok(u),
            // javascript:, data: и пр. пока не выполняем
            _ => {}
        }
    }

    // Похоже на домен? example.com / sub.example.org:8080/path
    let looks_like_domain = {
        let candidate = trimmed.split_whitespace().next().unwrap_or("");
        !candidate.contains(' ')
            && candidate.contains('.')
            && !candidate.starts_with('.')
            && !candidate.ends_with('.')
            && !candidate.contains('/')
            || (candidate.contains('.') && candidate.split('/').next().unwrap_or("").contains('.'))
    };

    if looks_like_domain {
        if let Ok(u) = Url::parse(&format!("https://{}", trimmed)) {
            return Ok(u);
        }
    }

    // Иначе — поиск
    let q = url::form_urlencoded::byte_serialize(trimmed.as_bytes()).collect::<String>();
    Url::parse(&format!("https://duckduckgo.com/?q={}", q))
        .map_err(|e| StrataError::InvalidUrl(e.to_string()))
}

/// Схема URL поддерживается движком?
pub fn is_supported(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https" | "file" | "about")
}

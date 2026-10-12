//! Загрузка тестовых фикстур из tests/fixtures/.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Одна тестовая фикстура: имя + HTML содержимое.
pub struct Fixture {
    pub name: &'static str,
    pub html: &'static str,
}

/// Статически вкомпилированные фикстуры. Делаем include_str! на
/// compile-time, чтобы бенчмарки не зависели от порядка файлов на диске.
pub static FIXTURES: &[Fixture] = &[
    Fixture {
        name: "simple",
        html: include_str!("../../../tests/fixtures/simple.html"),
    },
    Fixture {
        name: "text-heavy",
        html: include_str!("../../../tests/fixtures/text-heavy.html"),
    },
    Fixture {
        name: "flexbox",
        html: include_str!("../../../tests/fixtures/flexbox.html"),
    },
    Fixture {
        name: "colors",
        html: include_str!("../../../tests/fixtures/colors.html"),
    },
    Fixture {
        name: "nested",
        html: include_str!("../../../tests/fixtures/nested.html"),
    },
    Fixture {
        name: "acid2",
        html: include_str!("../../../tests/fixtures/acid2.html"),
    },
    Fixture {
        name: "forms",
        html: include_str!("../../../tests/fixtures/forms.html"),
    },
    Fixture {
        name: "semantics",
        html: include_str!("../../../tests/fixtures/semantics.html"),
    },
    Fixture {
        name: "entities",
        html: include_str!("../../../tests/fixtures/entities.html"),
    },
    Fixture {
        name: "selectors",
        html: include_str!("../../../tests/fixtures/selectors.html"),
    },
];

/// Найти фикстуру по имени.
pub fn find(name: &str) -> Option<&'static Fixture> {
    FIXTURES.iter().find(|f| f.name == name)
}

/// Путь к директории с тестовыми скриншотами.
pub fn screenshots_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests")
        .join("screenshots")
}

/// Путь к директории с эталонными скриншотами Chromium.
pub fn reference_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests")
        .join("reference")
}

/// Директория для отчётов о diff.
pub fn diff_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests")
        .join("diff")
}

/// Путь к tests/.
pub fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests")
}

/// Извлечь CSS из <style> тегов фикстуры (для бенчмарков).
pub fn extract_css(html: &str) -> String {
    // Простая реализация — для бенчмарков достаточно.
    let mut out = String::new();
    let mut pos = 0;
    let bytes = html.as_bytes();
    while pos < bytes.len() {
        if let Some(start) = html[pos..].find("<style>") {
            let abs_start = pos + start + "<style>".len();
            if let Some(end) = html[abs_start..].find("</style>") {
                out.push_str(&html[abs_start..abs_start + end]);
                out.push('\n');
                pos = abs_start + end + "</style>".len();
                continue;
            }
        }
        break;
    }
    out
}

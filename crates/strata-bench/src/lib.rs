//! Strata bench — бенчмарки и тесты производительности движка.
//!
//! Запуск:
//!   cargo bench -p strata-bench
//!   cargo bench -p strata-bench -- --save-baseline current
//!   cargo bench -p strata-bench -- html_parse
//!
//! Все бенчмарки используют фикстуры из tests/fixtures/.

pub mod fixtures;

pub use fixtures::Fixture;

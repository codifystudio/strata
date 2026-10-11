//! Единая система ошибок Strata.

use thiserror::Error;

pub type StrataResult<T> = Result<T, StrataError>;

#[derive(Error, Debug)]
pub enum StrataError {
    #[error("network error: {0}")]
    Network(String),

    #[error("invalid url: {0}")]
    InvalidUrl(String),

    #[error("html parse error: {0}")]
    HtmlParse(String),

    #[error("css parse error: {0}")]
    CssParse(String),

    #[error("layout error: {0}")]
    Layout(String),

    #[error("paint error: {0}")]
    Paint(String),

    #[error("js error: {0}")]
    Js(String),

    #[error("window error: {0}")]
    Window(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("other: {0}")]
    Other(String),
}

impl From<url::ParseError> for StrataError {
    fn from(e: url::ParseError) -> Self {
        StrataError::InvalidUrl(e.to_string())
    }
}

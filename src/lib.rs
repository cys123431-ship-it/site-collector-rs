//! 사이트 수집기 라이브러리
//!
//! 바이너리(`src/main.rs`)와 테스트가 함께 사용한다.

pub mod config;
pub mod crawler;
pub mod detect;
pub mod fetch;
pub mod filter;
pub mod font;
pub mod gui;
pub mod js_engine;
pub mod pipeline;
pub mod report;
pub mod state;
pub mod telegram;

/// 테스트에서 쓰는 내부 헬퍼 재노출
pub mod detect_helpers {
    pub use crate::crawler::{build_detail_url, build_page_url, pattern_matches};
    pub use crate::detect::patternize;
}

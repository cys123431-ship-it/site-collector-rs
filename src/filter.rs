//! 수집 결과 데이터 구조 및 키워드 필터

use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::config::KeywordMode;

/// 수집된 게시글 1건
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub site_id: String,
    pub site_name: String,
    pub title: String,
    pub url: String,
    /// 원본 문서의 날짜 표기 (예: "2026-10-29", "10.29")
    pub date_text: String,
    /// 파싱된 날짜 (없으면 None)
    pub date: Option<NaiveDate>,
    pub body: String,
    /// 키워드에 걸린 목록
    pub matched_keywords: Vec<String>,
}

impl Post {
    pub fn uid(&self) -> String {
        format!("{}:{}", self.site_id, self.url)
    }

    pub fn date_label(&self) -> String {
        match self.date {
            Some(d) => d.format("%Y-%m-%d").to_string(),
            None => {
                if self.date_text.is_empty() {
                    "날짜없음".to_string()
                } else {
                    self.date_text.clone()
                }
            }
        }
    }

    pub fn search_text(&self) -> String {
        format!("{}\n{}", self.title, self.body)
    }
}

/// 키워드 매칭 (대소문자 무시)
pub fn match_keywords(text: &str, keywords: &[String]) -> Vec<String> {
    let lower = text.to_lowercase();
    keywords
        .iter()
        .filter(|k| {
            let kw = k.trim().to_lowercase();
            !kw.is_empty() && lower.contains(&kw)
        })
        .cloned()
        .collect()
}

/// 사이트별 키워드 필터 적용
///
/// 사용자가 사이트에 입력한 키워드는 1차 필터 역할을 한다.
/// site.keywords 가 비어 있으면 전부 통과시킨다.
pub fn filter_by_site_keywords(
    posts: Vec<Post>,
    keywords: &[String],
) -> Vec<Post> {
    if keywords.is_empty() {
        return posts;
    }
    posts
        .into_iter()
        .filter_map(|mut post| {
            let hits = match_keywords(&post.search_text(), keywords);
            if hits.is_empty() {
                None
            } else {
                post.matched_keywords = hits;
                Some(post)
            }
        })
        .collect()
}

/// 알림/저장용 키워드 모드 필터 (matched_keywords 갱신 포함)
pub fn apply_keyword_mode(
    posts: Vec<Post>,
    mode: KeywordMode,
    keywords: &[String],
) -> Vec<Post> {
    let custom: Vec<String> = keywords
        .iter()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .collect();

    if mode == KeywordMode::All || custom.is_empty() {
        return posts;
    }

    let defaults = crate::config::default_keywords();

    let mut out = Vec::new();
    for mut post in posts {
        let text = post.search_text();
        let custom_hits = match_keywords(&text, &custom);
        let default_hits = match_keywords(&text, &defaults);

        let pass = match mode {
            KeywordMode::All => true,
            KeywordMode::KeywordOnly => !custom_hits.is_empty(),
            KeywordMode::DefaultOrKeyword => !custom_hits.is_empty() || !default_hits.is_empty(),
            KeywordMode::DefaultAndKeyword => !custom_hits.is_empty() && !default_hits.is_empty(),
        };

        if pass {
            for kw in custom_hits.into_iter().chain(default_hits) {
                if !post.matched_keywords.contains(&kw) {
                    post.matched_keywords.push(kw);
                }
            }
            out.push(post);
        }
    }
    out
}

/// 수집 실행 결과
#[derive(Debug, Default)]
pub struct RunResult {
    pub posts: Vec<Post>,
    pub errors: Vec<String>,
    pub started_at: String,
    pub finished_at: String,
    pub report_path: Option<String>,
    pub telegram_sent: bool,
}

impl RunResult {
    pub fn new() -> Self {
        Self {
            started_at: Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            ..Default::default()
        }
    }

    pub fn finish(&mut self) {
        self.finished_at = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    }

    pub fn count(&self) -> usize {
        self.posts.len()
    }
}

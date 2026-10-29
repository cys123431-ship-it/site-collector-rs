//! 수집 결과 리포트 생성 (Markdown)
//!
//! 경량 배포를 위해 docx 대신 Markdown 을 쓴다.
//! 워드/옵시디언/깃허브에서 모두 열린다.

use crate::filter::Post;
use anyhow::Result;
use chrono::{Local, NaiveDate};
use std::fs;
use std::path::{Path, PathBuf};

/// 리포트 파일명 생성
pub fn default_filename(start: NaiveDate, end: NaiveDate) -> String {
    if start == end {
        format!("수집리포트_{}.md", start.format("%Y-%m-%d"))
    } else {
        format!(
            "수집리포트_{}_{}.md",
            start.format("%Y-%m-%d"),
            end.format("%Y-%m-%d")
        )
    }
}

/// 마크다운 리포트를 생성한다.
pub fn build_markdown(
    posts: &[Post],
    start: NaiveDate,
    end: NaiveDate,
    include_body: bool,
    body_max_chars: usize,
    errors: &[String],
) -> String {
    let mut md = String::new();

    // ---- 머리말
    md.push_str("# 사이트 수집 리포트\n\n");
    md.push_str(&format!(
        "- **수집 기간**: {} ~ {}\n",
        start.format("%Y-%m-%d"),
        end.format("%Y-%m-%d")
    ));
    md.push_str(&format!("- **총 수집 건수**: {}건\n", posts.len()));
    md.push_str(&format!(
        "- **생성 시각**: {}\n\n",
        Local::now().format("%Y-%m-%d %H:%M:%S")
    ));

    if posts.is_empty() {
        md.push_str("해당 기간에 조건에 맞는 새 글이 없습니다.\n");
    } else {
        // ---- 사이트별 요약
        md.push_str("## 사이트별 요약\n\n");
        md.push_str("| 사이트 | 건수 |\n|---|---|\n");
        let mut counts: Vec<(String, usize)> = Vec::new();
        for post in posts {
            match counts.iter_mut().find(|(name, _)| name == &post.site_name) {
                Some((_, count)) => *count += 1,
                None => counts.push((post.site_name.clone(), 1)),
            }
        }
        for (name, count) in &counts {
            md.push_str(&format!("| {} | {}건 |\n", name, count));
        }
        md.push('\n');

        // ---- 전체 목록 (사이트별)
        md.push_str("## 전체 목록\n\n");
        let mut grouped: Vec<(String, Vec<&Post>)> = Vec::new();
        for post in posts {
            match grouped.iter_mut().find(|(name, _)| name == &post.site_name) {
                Some((_, list)) => list.push(post),
                None => grouped.push((post.site_name.clone(), vec![post])),
            }
        }

        for (site_name, list) in grouped {
            md.push_str(&format!("### {} ({}건)\n\n", site_name, list.len()));

            for (index, post) in list.iter().enumerate() {
                md.push_str(&format!(
                    "#### {}. {}\n\n",
                    index + 1,
                    escape_md(&post.title)
                ));
                md.push_str(&format!("- **등록일**: {}\n", post.date_label()));
                md.push_str(&format!(
                    "- **사이트**: {}\n",
                    escape_md(&post.site_name)
                ));
                md.push_str(&format!("- **링크**: <{}>\n", post.url));
                if !post.matched_keywords.is_empty() {
                    md.push_str(&format!(
                        "- **키워드**: {}\n",
                        post.matched_keywords.join(", ")
                    ));
                }
                md.push('\n');

                if include_body && !post.body.is_empty() {
                    let body = if post.body.chars().count() > body_max_chars {
                        let truncated: String = post.body.chars().take(body_max_chars).collect();
                        format!("{}\n\n...(생략)", truncated)
                    } else {
                        post.body.clone()
                    };
                    for line in body.lines() {
                        md.push_str(&format!("> {}\n", line));
                    }
                    md.push('\n');
                }
                md.push_str("---\n\n");
            }
        }
    }

    // ---- 오류
    if !errors.is_empty() {
        md.push_str("## 수집 중 오류\n\n");
        for err in errors {
            md.push_str(&format!("- {}\n", escape_md(err)));
        }
        md.push('\n');
    }

    md
}

/// 마크다운 특수문자 이스케이프 (최소한만)
fn escape_md(text: &str) -> String {
    text.replace('|', "\\|")
        .replace('\n', " ")
        .trim()
        .to_string()
}

/// 리포트를 저장한다.
pub fn save_report(
    dir: &Path,
    filename: &str,
    content: &str,
) -> Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join(filename);
    fs::write(&path, content)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn sample_post() -> Post {
        Post {
            site_id: "s1".to_string(),
            site_name: "테스트 사이트".to_string(),
            title: "2026 자격증 시험 안내".to_string(),
            url: "https://example.com/1".to_string(),
            date_text: "2026-10-29".to_string(),
            date: NaiveDate::from_ymd_opt(2026, 10, 29),
            body: "본문 내용입니다.".to_string(),
            matched_keywords: vec!["자격증".to_string()],
        }
    }

    #[test]
    fn generates_markdown() {
        let posts = vec![sample_post()];
        let start = NaiveDate::from_ymd_opt(2026, 10, 29).unwrap();
        let md = build_markdown(&posts, start, start, true, 4000, &[]);

        assert!(md.contains("# 사이트 수집 리포트"));
        assert!(md.contains("총 수집 건수**: 1건"));
        assert!(md.contains("2026 자격증 시험 안내"));
        assert!(md.contains("자격증"));
    }

    #[test]
    fn empty_report() {
        let start = NaiveDate::from_ymd_opt(2026, 10, 29).unwrap();
        let md = build_markdown(&[], start, start, true, 4000, &[]);
        assert!(md.contains("조건에 맞는 새 글이 없습니다"));
    }

    #[test]
    fn truncates_body() {
        let mut post = sample_post();
        post.body = "가".repeat(100);
        let start = NaiveDate::from_ymd_opt(2026, 10, 29).unwrap();
        let md = build_markdown(&[post], start, start, true, 10, &[]);
        assert!(md.contains("...(생략)"));
    }

    #[test]
    fn filename_single_day() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 29).unwrap();
        assert_eq!(default_filename(d, d), "수집리포트_2026-10-29.md");
    }
}

//! 감지된 구조를 이용한 실제 수집
//!
//! 두 가지 링크 방식을 모두 지원한다.
//!   - href 기반   : <a href="/notice/123">제목</a>
//!   - onclick 기반: <a onclick="article.view('123')">제목</a>
//!
//! onclick 은 상세 URL 을 알 수 없으므로 detail_url_template 을 사용한다.
//!   예) "{base}/{id}"  ->  https://site/notice/123

use crate::config::{CrawlConfig, SiteSource};
use crate::detect::{is_in_nav, node_text, PreviewItem};
use crate::fetch::Fetcher;
use crate::filter::Post;
use anyhow::Result;
use chrono::NaiveDate;
use scraper::{Html, Selector};
use std::collections::HashSet;

/// 연-월-일 우선, 없으면 월-일
fn date_regex() -> regex::Regex {
    regex::Regex::new(r"(20\d{2})[.\-/년]\s*(\d{1,2})[.\-/월]\s*(\d{1,2})|(\d{1,2})[.\-/](\d{1,2})")
        .expect("date regex")
}

/// 제목/날짜/링크를 기준으로 목록을 파싱한다.
pub fn parse_list(fetcher: &Fetcher, site: &SiteSource, page_url: &str) -> Result<Vec<PreviewItem>> {
    let html = fetcher.get(page_url)?;
    let doc = Html::parse_document(&html);

    let is_onclick = site.structure.link_pattern.starts_with("onclick:");
    let mut items: Vec<PreviewItem> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    if is_onclick {
        collect_onclick_items(&doc, site, page_url, &mut items, &mut seen);
    } else {
        collect_href_items(&doc, site, page_url, &mut items, &mut seen);
    }

    Ok(items)
}

/// href 기반 목록 수집
fn collect_href_items(
    doc: &Html,
    site: &SiteSource,
    page_url: &str,
    items: &mut Vec<PreviewItem>,
    seen: &mut HashSet<String>,
) {
    let Ok(sel) = Selector::parse("a[href]") else {
        return;
    };
    let date_re = date_regex();

    for anchor in doc.select(&sel) {
        let href = anchor.value().attr("href").unwrap_or("");
        let Some(abs) = absolutize(page_url, href) else {
            continue;
        };
        if !pattern_matches(&site.structure.link_pattern, href) {
            continue;
        }
        if !seen.insert(abs.clone()) {
            continue;
        }

        let title = anchor_title(&anchor);
        if title.is_empty() {
            continue;
        }

        let date_text = find_container_date(&anchor, &date_re).unwrap_or_default();
        items.push(PreviewItem {
            title,
            url: abs,
            date: date_text,
        });
    }
}

/// onclick 기반 목록 수집
fn collect_onclick_items(
    doc: &Html,
    site: &SiteSource,
    page_url: &str,
    items: &mut Vec<PreviewItem>,
    seen: &mut HashSet<String>,
) {
    let Ok(sel) = Selector::parse("a[onclick]") else {
        return;
    };
    let id_re = regex::Regex::new(r#"(?i)(?:view|detail|read|go)\w*\(\s*['"]?(\d+)['"]?"#)
        .expect("onclick id regex");
    let date_re = date_regex();
    let template = if site.structure.detail_url_template.is_empty() {
        "{base}/{id}"
    } else {
        site.structure.detail_url_template.as_str()
    };

    for anchor in doc.select(&sel) {
        let onclick = anchor.value().attr("onclick").unwrap_or("");
        let Some(caps) = id_re.captures(onclick) else {
            continue;
        };
        let Some(id_match) = caps.get(1) else {
            continue;
        };
        let id = id_match.as_str();

        // 내비게이션 영역 제외
        if is_in_nav(&anchor) {
            continue;
        }

        let url = build_detail_url(template, page_url, id);
        if !seen.insert(url.clone()) {
            continue;
        }

        let title = anchor_title(&anchor);
        if title.is_empty() {
            continue;
        }

        let date_text = find_container_date(&anchor, &date_re).unwrap_or_default();
        items.push(PreviewItem {
            title,
            url,
            date: date_text,
        });
    }
}

/// 상세 URL 템플릿에 값을 채운다.
fn build_detail_url(template: &str, page_url: &str, id: &str) -> String {
    let base = page_url.split('?').next().unwrap_or(page_url);
    let base = base.trim_end_matches('/');
    template
        .replace("{base}", base)
        .replace("{id}", id)
        .replace("{url}", page_url)
}

/// 앵커의 제목을 뽑는다. (텍스트 -> strong/span -> title 속성 순)
fn anchor_title(anchor: &scraper::ElementRef) -> String {
    let text = node_text(anchor);
    if text.chars().count() >= 2 {
        return text;
    }
    if let Ok(sel) = Selector::parse("strong, span, em") {
        for el in anchor.select(&sel) {
            let t = node_text(&el);
            if t.chars().count() >= 2 {
                return t;
            }
        }
    }
    anchor.value().attr("title").unwrap_or("").trim().to_string()
}

/// 앵커의 컨테이너에서 날짜를 찾는다. (li/tr/article 우선)
fn find_container_date(anchor: &scraper::ElementRef, date_re: &regex::Regex) -> Option<String> {
    let mut node = anchor.parent();
    let mut depth = 0;
    let mut fallback: Option<String> = None;

    while let Some(p) = node {
        if let Some(el) = scraper::ElementRef::wrap(p) {
            let name = el.value().name();
            let is_item = name == "li" || name == "tr" || name == "article";

            if let Some(found) = find_date_element(&el, date_re) {
                if is_item {
                    return Some(found);
                }
                if fallback.is_none() {
                    fallback = Some(found);
                }
            }

            if is_item {
                break;
            }
        }
        node = p.parent();
        depth += 1;
        if depth > 10 {
            break;
        }
    }
    fallback
}

/// 요소 안의 날짜 전용 클래스를 먼저 보고, 없으면 전체 텍스트에서 찾는다.
fn find_date_element(el: &scraper::ElementRef, date_re: &regex::Regex) -> Option<String> {
    let date_classes = [
        ".date",
        ".regDate",
        ".reg_date",
        ".date_txt",
        ".txt_date",
        ".write_date",
        ".created",
        ".created_at",
        ".datetime",
        "time",
        "[class*=date]",
    ];
    for css in date_classes {
        let Ok(sel) = Selector::parse(css) else {
            continue;
        };
        for child in el.select(&sel) {
            let text = node_text(&child);
            if text.is_empty() || text.chars().count() > 32 {
                continue;
            }
            if let Some(m) = date_re.find(&text) {
                return Some(m.as_str().to_string());
            }
        }
    }

    // 연도 포함 날짜를 우선 (제목의 "2026-2학기" 같은 오인 방지)
    let full = node_text(el);
    let year_re =
        regex::Regex::new(r"(20\d{2})[.\-/년]\s*(\d{1,2})[.\-/월]\s*(\d{1,2})").expect("year regex");
    if let Some(m) = year_re.find(&full) {
        return Some(m.as_str().to_string());
    }
    date_re.find(&full).map(|m| m.as_str().to_string())
}

/// 감지된 패턴(예: "/notice/{id}")과 실제 href 가 일치하는지
fn pattern_matches(pattern: &str, href: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }
    let (before, after) = match pattern.split_once("{id}") {
        Some(parts) => parts,
        None => return href.contains(pattern),
    };
    let Some(starts) = href.find(before) else {
        return false;
    };
    let rest_start = starts + before.len();
    if rest_start > href.len() {
        return false;
    }
    let rest = &href[rest_start..];
    let digit_count = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_count == 0 {
        return false;
    }
    if after.is_empty() {
        return true;
    }
    rest[digit_count..].contains(after)
}

fn absolutize(base: &str, href: &str) -> Option<String> {
    let href = href.trim();
    if href.is_empty() || href.starts_with("javascript:") || href.starts_with('#') {
        return None;
    }
    if href.starts_with("http://") || href.starts_with("https://") {
        return Some(href.to_string());
    }
    url::Url::parse(base).ok()?.join(href).ok().map(|u| u.to_string())
}

/// 상세 페이지에서 본문을 추출한다.
pub fn fetch_body(fetcher: &Fetcher, body_selector: &str, url: &str) -> Result<String> {
    let html = fetcher.get(url)?;
    let doc = Html::parse_document(&html);

    if !body_selector.is_empty() {
        if let Ok(sel) = Selector::parse(body_selector) {
            if let Some(el) = doc.select(&sel).next() {
                let text = extract_text(&el);
                if text.chars().count() >= 20 {
                    return Ok(text);
                }
            }
        }
    }

    // 폴백: 텍스트가 가장 긴 요소
    let Ok(sel) = Selector::parse("div, article, section") else {
        return Ok(String::new());
    };
    let mut best = String::new();
    for el in doc.select(&sel) {
        let name = format!(
            "{} {}",
            el.value().attr("id").unwrap_or(""),
            el.value().attr("class").unwrap_or("")
        )
        .to_lowercase();
        if ["nav", "footer", "header", "menu", "sidebar", "comment"]
            .iter()
            .any(|k| name.contains(k))
        {
            continue;
        }
        let text = extract_text(&el);
        if text.chars().count() > best.chars().count() {
            best = text;
        }
    }
    Ok(best)
}

/// 요소에서 줄바꿈을 살려 텍스트 추출
fn extract_text(el: &scraper::ElementRef) -> String {
    let inner = el.inner_html();
    let with_breaks = regex::Regex::new(r"(?i)<(br|/p|/div|/li|/tr|/h[1-6])\s*/?>")
        .expect("break regex")
        .replace_all(&inner, "$0\n")
        .to_string();

    let fragment = Html::parse_fragment(&with_breaks);
    let raw = fragment.root_element().text().collect::<Vec<_>>().join("");

    let lines: Vec<String> = raw
        .split('\n')
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();

    let mut result: Vec<String> = Vec::new();
    let mut prev_blank = false;
    for line in lines {
        if line.is_empty() {
            if prev_blank {
                continue;
            }
            prev_blank = true;
        } else {
            prev_blank = false;
        }
        result.push(line);
    }
    result.join("\n").trim().to_string()
}

/// 날짜 문자열을 NaiveDate 로 파싱 (연도 없으면 올해로 가정)
pub fn parse_date(text: &str, reference: NaiveDate) -> Option<NaiveDate> {
    use chrono::Datelike;
    let re = date_regex();
    let caps = re.captures(text)?;

    // ① 연-월-일
    if let (Some(y), Some(m), Some(d)) = (caps.get(1), caps.get(2), caps.get(3)) {
        let year: i32 = y.as_str().parse().ok()?;
        let month: u32 = m.as_str().parse().ok()?;
        let day: u32 = d.as_str().parse().ok()?;
        if (1900..=2200).contains(&year) && (1..=12).contains(&month) && (1..=31).contains(&day) {
            return NaiveDate::from_ymd_opt(year, month, day);
        }
        return None;
    }

    // ② 월-일
    if let (Some(m), Some(d)) = (caps.get(4), caps.get(5)) {
        let month: u32 = m.as_str().parse().ok()?;
        let day: u32 = d.as_str().parse().ok()?;
        if (1..=12).contains(&month) && (1..=31).contains(&day) {
            return NaiveDate::from_ymd_opt(reference.year(), month, day);
        }
    }
    None
}

/// 한 사이트를 수집한다
pub fn collect_site(
    fetcher: &Fetcher,
    site: &SiteSource,
    crawl: &CrawlConfig,
    since: NaiveDate,
    until: NaiveDate,
    fetch_body_text: bool,
) -> Result<Vec<Post>> {
    let today = chrono::Local::now().date_naive();
    let mut posts: Vec<Post> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for page in 1..=crawl.max_pages {
        let page_url = build_page_url(&site.url, &site.structure.page_pattern, page);
        let items = match parse_list(fetcher, site, &page_url) {
            Ok(items) => items,
            Err(err) => {
                if page == 1 {
                    return Err(err);
                }
                break;
            }
        };
        if items.is_empty() {
            break;
        }

        let mut found_old = false;
        for item in items {
            if !seen.insert(item.url.clone()) {
                continue;
            }
            let Some(date) = parse_date(&item.date, today) else {
                continue;
            };
            if date < since {
                found_old = true;
                continue;
            }
            if date > until {
                continue;
            }

            let body = if fetch_body_text {
                fetch_body(fetcher, &site.structure.body_selector, &item.url).unwrap_or_default()
            } else {
                String::new()
            };

            posts.push(Post {
                site_id: site.id.clone(),
                site_name: site.name.clone(),
                title: item.title,
                url: item.url,
                date_text: item.date,
                date: Some(date),
                body,
                matched_keywords: Vec::new(),
            });
        }

        // 최신순 목록이면 예전 글이 나온 시점에서 중단
        if found_old {
            break;
        }
    }

    Ok(posts)
}

/// 페이지 번호를 넣은 URL 생성
fn build_page_url(base: &str, pattern: &str, page: u32) -> String {
    if page <= 1 || pattern.is_empty() {
        return base.to_string();
    }
    let filled = pattern
        .replace("{n}", &page.to_string())
        .replace("{page}", &page.to_string());

    if filled.starts_with('?') || filled.starts_with('/') {
        format!("{}{}", base.trim_end_matches('/'), filled)
    } else {
        format!("{}/{}", base.trim_end_matches('/'), filled.trim_start_matches('/'))
    }
}

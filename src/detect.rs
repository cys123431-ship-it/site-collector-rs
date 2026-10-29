//! 자동 구조 감지 엔진
//!
//! 주소 하나를 받아서 "이 페이지가 게시판 목록인가?" 를 판단하고,
//! 제목/날짜/링크/본문 위치를 추론한다.
//!
//! 동작 순서
//!   1) HTML 수집
//!   2) <a> 링크를 href 패턴별로 그룹화
//!   3) 반복되는 패턴 = 게시글 링크 후보
//!   4) 부모 요소에서 날짜를 찾아 게시판 여부 확정
//!   5) 제목 추출 규칙 결정
//!   6) 상세 1건을 열어 본문 위치 추정
//!   7) 페이지 이동 패턴 추출

use crate::config::{DetectedStructure, SiteSource};
use crate::fetch::Fetcher;
use anyhow::{anyhow, Result};
use scraper::{Html, Selector};
use std::collections::HashMap;

/// 감지된 게시글 1건 (미리보기용)
#[derive(Debug, Clone)]
pub struct PreviewItem {
    pub title: String,
    pub url: String,
    pub date: String,
}

/// 감지 결과
#[derive(Debug, Clone)]
pub struct DetectResult {
    pub ok: bool,
    pub needs_js: bool,
    pub message: String,
    pub structure: DetectedStructure,
    pub items: Vec<PreviewItem>,
    pub item_count: usize,
    pub date_found: bool,
    pub body_found: bool,
}

/// 날짜 형태를 찾는 정규식
/// (Rust regex 는 look-around 를 지원하지 않으므로 단순 패턴만 사용)
fn date_regex() -> regex::Regex {
    regex::Regex::new(
        r"(20\d{2})[.\-/년]\s*(\d{1,2})[.\-/월]\s*(\d{1,2})|(\d{1,2})[.\-/](\d{1,2})",
    )
    .expect("date regex")
}

/// 링크를 패턴화한다. 2자리 이상 숫자를 {id} 로 치환.
fn patternize(href: &str) -> String {
    let mut out = String::with_capacity(href.len());
    let mut digits = 0usize;
    for ch in href.chars() {
        if ch.is_ascii_digit() {
            digits += 1;
            continue;
        }
        if digits > 0 {
            if digits >= 2 {
                out.push_str("{id}");
            } else {
                out.push('0');
            }
            digits = 0;
        }
        out.push(ch);
    }
    if digits > 0 {
        if digits >= 2 {
            out.push_str("{id}");
        } else {
            out.push('0');
        }
    }
    out
}

/// 절대 URL 로 변환
fn absolutize(base: &str, href: &str) -> Option<String> {
    let href = href.trim();
    if href.is_empty() || href.starts_with("javascript:") || href.starts_with('#') {
        return None;
    }
    if href.starts_with("http://") || href.starts_with("https://") {
        return Some(href.to_string());
    }
    let base_url = url::Url::parse(base).ok()?;
    base_url.join(href).ok().map(|u| u.to_string())
}

/// 요소의 텍스트를 공백 정리해서 반환
pub fn node_text(el: &scraper::ElementRef) -> String {
    el.text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn attr_title(el: &scraper::ElementRef) -> String {
    el.value().attr("title").unwrap_or("").trim().to_string()
}

/// 게시판 구조를 감지한다.
pub fn detect(fetcher: &Fetcher, page_url: &str) -> Result<DetectResult> {
    let html = fetcher.get(page_url)?;

    let looks_spa = Fetcher::looks_like_spa(&html);
    let doc = Html::parse_document(&html);

    // ---- 1) 링크 후보 수집 (href 패턴 + onclick 패턴)
    let a_sel = Selector::parse("a[href]").expect("a selector");
    let mut groups: HashMap<String, Vec<(String, String)>> = HashMap::new();

    for anchor in doc.select(&a_sel) {
        let href = anchor.value().attr("href").unwrap_or("");
        let Some(abs) = absolutize(page_url, href) else {
            continue;
        };
        let text = {
            let t = node_text(&anchor);
            if t.chars().count() >= 2 {
                t
            } else {
                attr_title(&anchor)
            }
        };
        if text.is_empty() {
            continue;
        }
        groups.entry(patternize(href)).or_default().push((abs, text));
    }

    // onclick="article.view('123')" 형태 (많은 CMS 게시판이 이 방식)
    let onclick_re = regex::Regex::new(r"(?i)(?:view|detail|read|go)\w*\(\s*'?(\d+)'?")
        .expect("onclick regex");
    let mut onclick_groups: HashMap<String, Vec<(String, String)>> = HashMap::new();

    for anchor in doc.select(&a_sel) {
        let onclick = anchor.value().attr("onclick").unwrap_or("");
        if onclick.is_empty() {
            continue;
        }
        let Some(caps) = onclick_re.captures(onclick) else {
            continue;
        };
        let id = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        if id.is_empty() {
            continue;
        }
        let text = {
            let t = node_text(&anchor);
            if t.chars().count() >= 2 {
                t
            } else {
                attr_title(&anchor)
            }
        };
        if text.is_empty() {
            continue;
        }
        // onclick 함수 이름을 패턴으로 사용
        let func = onclick
            .split('(')
            .next()
            .unwrap_or("view")
            .trim()
            .to_string();
        onclick_groups
            .entry(format!("onclick:{}", func))
            .or_default()
            .push((format!("{}#{}", page_url, id), text));
    }

    // onclick 그룹이 더 많으면 우선 사용
    for (pattern, items) in onclick_groups {
        if items.len() >= 5 {
            groups.entry(pattern).or_default().extend(items);
        }
    }

    // ---- 2) 패턴별 점수 계산
    //      - 날짜가 함께 있는 항목이 많을수록 게시판일 확률이 높다
    //      - 메뉴/내비 영역은 제외한다
    let date_re = date_regex();
    let mut best: Option<(String, Vec<(String, String)>, usize)> = None;

    for (pattern, items) in groups {
        if items.len() < 5 {
            continue;
        }
        let avg_len = items.iter().map(|(_, t)| t.chars().count()).sum::<usize>() / items.len();
        if avg_len < 6 {
            continue;
        }

        // 날짜가 붙은 항목 수 계산
        let mut dated = 0usize;
        let mut previews: Vec<(String, String, String)> = Vec::new();

        for (abs, text) in items.iter().take(40) {
            let Some(anchor) = find_anchor(&doc, abs, page_url) else {
                continue;
            };

            // 내비게이션 영역(gnb/lnb/nav/menu/header/footer)에 있으면 제외
            if is_in_nav(&anchor) {
                continue;
            }

            let container = find_container(&anchor);
            let text_all = container.map(|c| node_text(&c)).unwrap_or_default();
            let date_text = date_re
                .find(&text_all)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            if !date_text.is_empty() {
                dated += 1;
            }
            previews.push((text.clone(), abs.clone(), date_text));
        }

        if previews.len() < 5 {
            continue;
        }

        // 점수: 날짜 있는 항목 수를 최우선으로
        let score = dated * 100 + previews.len();

        let better = match &best {
            None => true,
            Some((_, _, best_score)) => score > *best_score,
        };

        if better {
            // 저장 순서는 (url, title) — 나머지 코드와 맞춘다
            let pairs: Vec<(String, String)> = previews
                .iter()
                .map(|(t, u, _)| (u.clone(), t.clone()))
                .collect();
            best = Some((pattern, pairs, score));
        }
    }

    let Some((link_pattern, links, _score)) = best else {
        let message = if looks_spa {
            "JavaScript 렌더링이 필요한 페이지로 보입니다. 옵션 엔진을 설치한 뒤 'JS 사용'을 켜고 다시 분석하세요."
        } else {
            "게시글 목록을 찾지 못했습니다. 게시판 목록 페이지 주소인지 확인해 주세요."
        };
        return Ok(DetectResult {
            ok: false,
            needs_js: looks_spa,
            message: message.to_string(),
            structure: DetectedStructure::default(),
            items: Vec::new(),
            item_count: 0,
            date_found: false,
            body_found: false,
        });
    };

    // ---- 3) 날짜 탐색 + 미리보기 생성
    let date_re = date_regex();
    let mut dated = 0usize;
    let mut previews: Vec<PreviewItem> = Vec::new();
    let mut date_selector_hint = String::new();

    // onclick 패턴이면 별도 처리 (li 컨테이너 안에 .date 가 있는 구조)
    let is_onclick = link_pattern.starts_with("onclick:");

    for (abs, text) in links.iter().take(30) {
        let date_text = if is_onclick {
            let id = abs.rsplit_once('#').map(|(_, id)| id).unwrap_or("");
            let anchor = find_onclick_anchor(&doc, id);
            let container = anchor.as_ref().and_then(find_container);
            match container.and_then(|c| find_date_in_container(&c, &date_re)) {
                Some((date, hint_el)) => {
                    dated += 1;
                    if date_selector_hint.is_empty() {
                        if let Some(el) = hint_el {
                            date_selector_hint = css_hint(&el);
                        }
                    }
                    date
                }
                None => String::new(),
            }
        } else {
            let Some(anchor) = find_anchor(&doc, abs, page_url) else {
                continue;
            };
            let container = find_container(&anchor);
            match container.and_then(|c| find_date_in_container(&c, &date_re)) {
                Some((date, hint_el)) => {
                    dated += 1;
                    if date_selector_hint.is_empty() {
                        if let Some(el) = hint_el {
                            date_selector_hint = css_hint(&el);
                        }
                    }
                    date
                }
                None => String::new(),
            }
        };

        previews.push(PreviewItem {
            title: text.clone(),
            url: abs.clone(),
            date: date_text,
        });
    }

    let date_found = dated >= 2;

    // ---- 4) 상세 페이지에서 본문 위치 추정
    //      onclick 패턴이면 URL 추론 필요: 목록 주소 + "/{id}" 형태를 시도
    let (body_selector, body_found, detail_template) = match links.first() {
        Some((url, _)) => {
            if url.contains('#') {
                let id = url.rsplit_once('#').map(|(_, i)| i).unwrap_or("");
                let base = page_url.split('?').next().unwrap_or(page_url);
                let base = base.trim_end_matches('/');
                let candidates = [
                    ("{base}/{id}", format!("{}/{}", base, id)),
                    ("{base}/view/{id}", format!("{}/view/{}", base, id)),
                    ("{base}?articleNo={id}", format!("{}?articleNo={}", base, id)),
                ];
                let mut found = (String::new(), false, "{base}/{id}".to_string());
                for (template, candidate) in candidates {
                    if let Ok((sel, ok)) = detect_body(fetcher, &candidate) {
                        if ok {
                            found = (sel, true, template.to_string());
                            break;
                        }
                    }
                }
                found
            } else {
                let (sel, ok) = detect_body(fetcher, url).unwrap_or_default();
                (sel, ok, String::new())
            }
        }
        None => (String::new(), false, String::new()),
    };

    // ---- 5) 페이지 이동 패턴
    let page_pattern = detect_page_pattern(&doc);

    // ---- 6) 결과 메시지
    let mut message = format!("게시글 {}건 감지", links.len());
    message.push_str(if date_found { " · 날짜 인식" } else { " · 날짜 미인식" });
    message.push_str(if body_found { " · 본문 인식" } else { " · 본문 미인식" });
    if !page_pattern.is_empty() {
        message.push_str(" · 페이지 이동 인식");
    }
    if looks_spa {
        message.push_str(" (JS 필요 가능성)");
    }

    let structure = DetectedStructure {
        link_pattern,
        item_selector: "li, tr, article".to_string(),
        title_selector: "a".to_string(),
        date_selector: date_selector_hint,
        body_selector,
        page_pattern,
        detail_url_template: detail_template,
        body_detected: body_found,
    };

    Ok(DetectResult {
        ok: true,
        needs_js: looks_spa,
        message,
        structure,
        items: previews.into_iter().take(10).collect(),
        item_count: links.len(),
        date_found,
        body_found,
    })
}

/// onclick 속성에서 특정 id 를 가진 앵커 찾기 (내비 영역 제외)
fn find_onclick_anchor<'a>(doc: &'a Html, id: &str) -> Option<scraper::ElementRef<'a>> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let sel = Selector::parse("a[onclick]").ok()?;
    let needle = format!("'{}'", id);
    let needle_dq = format!("\"{}\"", id);

    let mut fallback: Option<scraper::ElementRef> = None;
    for a in doc.select(&sel) {
        let onclick = a.value().attr("onclick").unwrap_or("");
        if !onclick.contains(&needle) && !onclick.contains(&needle_dq) {
            continue;
        }
        // 내비 영역이 아닌 것을 우선 반환
        if !is_in_nav(&a) {
            return Some(a);
        }
        if fallback.is_none() {
            fallback = Some(a);
        }
    }
    fallback
}

/// 문서에서 절대 URL 과 일치하는 a 태그 찾기
fn find_anchor<'a>(doc: &'a Html, abs: &str, page_url: &str) -> Option<scraper::ElementRef<'a>> {
    // onclick 패턴("URL#id")은 별도로 처리
    if let Some(id) = abs.rsplit_once('#').map(|(_, id)| id) {
        if abs.starts_with(page_url) && !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
            let sel = Selector::parse("a[onclick]").ok()?;
            let re = regex::Regex::new(&format!(r#"['"]{}['"]"#, regex::escape(id))).ok()?;
            for a in doc.select(&sel) {
                let onclick = a.value().attr("onclick").unwrap_or("");
                if re.is_match(onclick) {
                    return Some(a);
                }
            }
            return None;
        }
    }

    let sel = Selector::parse("a[href]").ok()?;
    for a in doc.select(&sel) {
        let href = a.value().attr("href").unwrap_or("");
        if let Some(candidate) = absolutize(page_url, href) {
            if candidate == abs {
                return Some(a);
            }
        }
    }
    None
}

/// 내비게이션/메뉴 영역 안에 있는 링크인지 판단한다.
pub fn is_in_nav(anchor: &scraper::ElementRef) -> bool {
    let mut node = anchor.parent();
    let mut depth = 0;
    while let Some(p) = node {
        if let Some(el) = scraper::ElementRef::wrap(p) {
            let name = el.value().name();
            // nav 태그는 즉시 제외
            if name == "nav" || name == "header" || name == "footer" {
                return true;
            }
            let id = el.value().attr("id").unwrap_or("").to_lowercase();
            let class = el.value().attr("class").unwrap_or("").to_lowercase();
            let marker = format!("{} {}", id, class);
            let nav_words = [
                "gnb", "lnb", "nav", "menu", "header", "footer", "sidebar", "snb",
                "breadcrumb", "breadscrumb", "topmenu", "mainmenu", "submenu", "sitemap",
            ];
            // 게시판 목록 영역은 내비가 아니다
            let board_words = ["board", "notice_list", "list", "bbs", "gallery", "content"];
            if board_words.iter().any(|w| marker.contains(w)) {
                // 단, nav/menu 가 함께 있으면 내비로 판단
                if !nav_words.iter().any(|w| marker.contains(w)) {
                    return false;
                }
            }
            if nav_words.iter().any(|w| marker.contains(w)) {
                return true;
            }
        }
        node = p.parent();
        depth += 1;
        if depth > 8 {
            break;
        }
    }
    false
}

/// 링크의 컨테이너(li/tr/article 우선) 찾기
fn find_container<'a>(anchor: &scraper::ElementRef<'a>) -> Option<scraper::ElementRef<'a>> {
    let mut node = anchor.parent();
    let mut fallback: Option<scraper::ElementRef> = None;
    let mut depth = 0;

    while let Some(p) = node {
        if let Some(el) = scraper::ElementRef::wrap(p) {
            let name = el.value().name();
            // li/tr/article 은 즉시 컨테이너로 확정
            if name == "li" || name == "tr" || name == "article" {
                return Some(el);
            }
            if fallback.is_none() && (name == "div" || name == "section" || name == "p") {
                fallback = Some(el);
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

/// 컨테이너에서 날짜 텍스트를 찾는다.
/// 제목에 섞인 숫자(예: "2026-2학기")를 날짜로 오인하지 않도록
/// ① 날짜 관련 클래스를 가진 요소를 먼저 보고 ② 그다음 전체 텍스트를 본다.
fn find_date_in_container<'a>(
    container: &scraper::ElementRef<'a>,
    date_re: &regex::Regex,
) -> Option<(String, Option<scraper::ElementRef<'a>>)> {
    // ① 날짜 클래스를 가진 자식 요소 우선 (.date, .regDate, .date_txt, time 등)
    let date_classes = [
        ".date", ".regDate", ".reg_date", ".date_txt", ".txt_date", ".write_date",
        ".created", ".created_at", ".datetime", "time", "[class*=date]",
    ];
    for css in date_classes {
        let Ok(sel) = Selector::parse(css) else {
            continue;
        };
        for child in container.select(&sel) {
            let text = node_text(&child);
            if text.is_empty() || text.chars().count() > 32 {
                continue;
            }
            // 완전한 날짜(연-월-일)를 우선
            if let Some(m) = date_re.find(&text) {
                return Some((m.as_str().to_string(), Some(child)));
            }
        }
    }

    // ② 전체 텍스트에서 찾되, 4자리 연도가 포함된 것을 우선
    let full_text = node_text(container);
    let year_re = regex::Regex::new(
        r"(20\d{2})[.\-/년]\s*(\d{1,2})[.\-/월]\s*(\d{1,2})",
    )
    .expect("year regex");
    if let Some(m) = year_re.find(&full_text) {
        return Some((m.as_str().to_string(), None));
    }

    // ③ 마지막으로 짧은 날짜 (10.23)
    if let Some(m) = date_re.find(&full_text) {
        return Some((m.as_str().to_string(), None));
    }
    None
}

/// 요소의 클래스/태그로 CSS 선택자 힌트를 만든다.
fn css_hint(el: &scraper::ElementRef) -> String {
    let class = el.value().attr("class").unwrap_or("");
    if let Some(first) = class.split_whitespace().next() {
        if !first.is_empty() {
            return format!(".{}", first);
        }
    }
    let name = el.value().name();
    if name == "time" {
        return "time".to_string();
    }
    String::new()
}

/// 상세 페이지에서 본문 요소의 CSS 선택자를 추정한다.
fn detect_body(fetcher: &Fetcher, url: &str) -> Result<(String, bool)> {
    let html = fetcher.get(url)?;
    let doc = Html::parse_document(&html);

    let candidates = [
        "article .entry-content",
        "div.entry-content",
        "div.post-content",
        "div.board_view .contents",
        "div.view-content",
        "div#bo_v_con",
        "div.contents_style",
        "#content .contents",
        "div.detail .contents",
        "article",
        "main",
        "div.content",
    ];

    for css in candidates {
        let Ok(sel) = Selector::parse(css) else {
            continue;
        };
        if let Some(el) = doc.select(&sel).next() {
            if node_text(&el).chars().count() >= 80 {
                return Ok((css.to_string(), true));
            }
        }
    }

    let Ok(sel) = Selector::parse("div, article, section") else {
        return Ok((String::new(), false));
    };
    let mut best_len = 0usize;
    let mut best_css = String::new();

    for el in doc.select(&sel) {
        let text = node_text(&el);
        let len = text.chars().count();
        let id = el.value().attr("id").unwrap_or("");
        let class = el.value().attr("class").unwrap_or("");
        let name = format!("{} {}", id, class).to_lowercase();
        if ["nav", "footer", "header", "menu", "sidebar", "comment", "paging", "gnb"]
            .iter()
            .any(|k| name.contains(k))
        {
            continue;
        }
        if len > best_len {
            best_len = len;
            let tag = el.value().name();
            best_css = if !id.is_empty() {
                format!("{}#{}", tag, id)
            } else if let Some(first) = class.split_whitespace().next() {
                format!(".{}", first)
            } else {
                tag.to_string()
            };
        }
    }

    if best_len >= 120 && !best_css.is_empty() {
        Ok((best_css, true))
    } else {
        Ok((String::new(), false))
    }
}

/// 페이지 이동 링크 패턴 추출 (예: "?page={n}")
fn detect_page_pattern(doc: &Html) -> String {
    let Ok(sel) = Selector::parse("a[href]") else {
        return String::new();
    };
    let param_re = regex::Regex::new(r"[?&]([a-zA-Z_]+)=(\d+)").expect("param regex");
    let path_re = regex::Regex::new(r"/page/(\d+)").expect("path regex");
    let known = ["page", "pageIndex", "pageNo", "pg", "curPage"];

    for a in doc.select(&sel) {
        let href = a.value().attr("href").unwrap_or("");
        let text = node_text(&a);
        let is_page_link = (!text.is_empty() && text.chars().all(|c| c.is_ascii_digit()))
            || text.contains("다음")
            || text.contains("next")
            || text == ">";
        if !is_page_link {
            continue;
        }
        if path_re.is_match(href) {
            return "/page/{n}".to_string();
        }
        for cap in param_re.captures_iter(href) {
            let param = cap[1].to_string();
            if known.iter().any(|k| k.eq_ignore_ascii_case(&param)) {
                return format!("?{}={{n}}", param);
            }
        }
    }
    String::new()
}

/// 텍스트에서 키워드에 걸린 것들을 반환 (대소문자 무시)
pub fn matches_keywords(text: &str, keywords: &[String]) -> Vec<String> {
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

/// 주소 기본 검증
pub fn validate_url(url: &str) -> Result<()> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("주소가 비어 있습니다."));
    }
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err(anyhow!("http:// 또는 https:// 로 시작해야 합니다."));
    }
    Ok(())
}

/// 사이트 정보 검증
pub fn validate_source(site: &SiteSource) -> Result<()> {
    validate_url(&site.url)
}

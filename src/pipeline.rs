//! 수집 파이프라인
//!
//! 설정 → 사이트별 수집 → 키워드 필터 → 중복 제거 → 리포트 → 텔레그램

use crate::config::{output_dir, Config};
use crate::crawler::collect_site;
use crate::fetch::Fetcher;
use crate::filter::{apply_keyword_mode, filter_by_site_keywords, Post};
use crate::report::{build_markdown, default_filename, save_report};
use crate::state::StateStore;
use crate::telegram;
use anyhow::Result;
use chrono::{Local, NaiveDate};

/// 실행 진행 상황을 GUI/CLI 로 전달하는 로그 함수
pub type LogFn = Box<dyn Fn(&str) + Send>;

/// 실행 결과
pub struct RunResult {
    pub posts: Vec<Post>,
    pub errors: Vec<String>,
    pub report_path: Option<String>,
    pub sent: bool,
    pub from: NaiveDate,
    pub to: NaiveDate,
}

/// 실행 옵션
#[derive(Debug, Clone)]
pub struct RunOptions {
    pub days: u32,
    pub make_report: bool,
    pub send_telegram: bool,
    pub include_seen: bool,
    pub fetch_body: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            days: 2,
            make_report: true,
            send_telegram: true,
            include_seen: false,
            fetch_body: true,
        }
    }
}

/// 전체 파이프라인 실행
pub fn run(config: &Config, options: &RunOptions, log: &LogFn) -> Result<RunResult> {
    let today = Local::now().date_naive();
    let from = today - chrono::Duration::days(options.days.saturating_sub(1) as i64);
    let to = today;

    log(&format!(
        "[기간] {} ~ {}",
        from.format("%Y-%m-%d"),
        to.format("%Y-%m-%d")
    ));

    let fetcher = Fetcher::new(
        &config.crawl.user_agent,
        config.crawl.timeout_secs,
        config.crawl.delay_ms,
        config.crawl.use_js_engine,
    )?;

    let mut all_posts: Vec<Post> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut state = StateStore::load();

    // ---- 사이트별 수집
    for site in config.sites.iter().filter(|s| s.enabled) {
        log(&format!("[수집] {}", site.name));
        match collect_site(
            &fetcher,
            site,
            &config.crawl,
            from,
            to,
            options.fetch_body && config.output.include_body,
        ) {
            Ok(posts) => {
                let total = posts.len();
                let filtered = filter_by_site_keywords(posts, &site.keywords);
                if site.keywords.is_empty() {
                    log(&format!("      {}건 수집", filtered.len()));
                } else {
                    log(&format!(
                        "      {}건 수집 -> 키워드 통과 {}건",
                        total,
                        filtered.len()
                    ));
                }
                all_posts.extend(filtered);
            }
            Err(err) => {
                log(&format!("      실패 - {}", err));
                errors.push(format!("{}: {}", site.name, err));
            }
        }
    }

    log(&format!("[수집] 총 {}건", all_posts.len()));

    // ---- 중복 제거
    if !options.include_seen {
        let before = all_posts.len();
        all_posts.retain(|p| !state.is_seen(&p.uid()));
        let removed = before - all_posts.len();
        if removed > 0 {
            log(&format!("[중복] 이미 처리한 글 {}건 제외", removed));
        }
    }

    // ---- 키워드 필터 (저장/알림 공통)
    let mut filtered = apply_keyword_mode(
        all_posts,
        config.telegram.keyword_mode,
        &config.telegram.notify_keywords,
    );
    filtered.sort_by(|a, b| b.date.cmp(&a.date));

    log(&format!("[필터] {}건 통과", filtered.len()));

    // ---- 리포트 생성
    let mut report_path = None;
    if options.make_report && !filtered.is_empty() {
        let md = build_markdown(
            &filtered,
            from,
            to,
            config.output.include_body,
            config.output.body_max_chars,
            &errors,
        );
        let dir = output_dir(config);
        let filename = default_filename(from, to);
        match save_report(&dir, &filename, &md) {
            Ok(path) => {
                log(&format!("[리포트] {}", path.display()));
                report_path = Some(path.to_string_lossy().to_string());
            }
            Err(err) => {
                log(&format!("[리포트] 저장 실패 - {}", err));
                errors.push(format!("리포트 저장 실패: {}", err));
            }
        }
    }

    // ---- 텔레그램 전송
    let mut sent = false;
    if options.send_telegram && config.telegram.enabled && !filtered.is_empty() {
        if state.paused() {
            log("[알림] 일시정지 상태입니다. (/resume 로 재개)");
        } else {
            let token = crate::config::telegram_token();
            let chat_id = config.telegram.chat_id.trim();
            if token.is_empty() || chat_id.is_empty() {
                log("[알림] 봇 토큰 또는 chat_id 가 없습니다.");
            } else {
                let header = format!(
                    "{} {} 새 글 {}건",
                    config.telegram.header_prefix,
                    to.format("%Y-%m-%d"),
                    filtered.len()
                );
                match telegram::send_posts(&token, chat_id, &filtered, &header) {
                    Ok(()) => {
                        log("[알림] 텔레그램 전송 완료");
                        sent = true;
                    }
                    Err(err) => {
                        log(&format!("[알림] 전송 실패 - {}", err));
                        errors.push(format!("텔레그램 전송 실패: {}", err));
                    }
                }
            }
        }
    }

    // ---- 이력 기록 (전송 성공 시에만 '처리됨'으로 표시)
    if sent || !options.send_telegram {
        let uids: Vec<String> = filtered.iter().map(|p| p.uid()).collect();
        state.mark_seen(&uids);
    }
    state.record_run(filtered.len(), sent, report_path.as_deref().unwrap_or(""));
    state.prune();
    if let Err(err) = state.save() {
        log(&format!("[이력] 저장 실패 - {}", err));
    }

    if filtered.is_empty() {
        log("[결과] 조건에 맞는 새 글이 없습니다.");
    }

    Ok(RunResult {
        posts: filtered,
        errors,
        report_path,
        sent,
        from,
        to,
    })
}

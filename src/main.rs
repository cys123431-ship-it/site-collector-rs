//! 범용 사이트 수집기 (Rust)
//!
//! 주소와 키워드를 넣으면 그 사이트에서 키워드에 걸리는 게시글을 수집한다.
//! 자동으로 게시판 구조를 감지하고, Markdown 리포트 + 텔레그램 알림을 제공한다.

mod config;
mod crawler;
mod detect;
mod fetch;
mod filter;
mod font;
mod gui;
mod js_engine;
mod pipeline;
mod report;
mod state;
mod telegram;

use anyhow::Result;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_target(false)
        .init();

    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(|s| s.as_str()).unwrap_or("gui");

    match command {
        "--detect" | "detect" => detect_command(&args),
        "--run" | "run" => run_command(&args),
        "--status" | "status" => status_command(),
        "--commands" | "commands" => commands_command(),
        "--version" | "-V" => {
            println!("site-collector {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "--help" | "-h" | "help" => {
            print_help();
            Ok(())
        }
        _ => run_gui(),
    }
}

/// GUI 실행
fn run_gui() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([900.0, 700.0])
            .with_min_inner_size([700.0, 500.0])
            .with_title("사이트 수집기"),
        ..Default::default()
    };

    eframe::run_native(
        "사이트 수집기",
        options,
        Box::new(|cc| {
            // 한글 폰트 등록 (없으면 네모로 표시됨)
            let installed = font::install_korean_font(&cc.egui_ctx);
            if let Some(path) = &installed {
                tracing::info!("한글 폰트 로드: {}", path.display());
            } else {
                tracing::warn!("한글 폰트를 찾지 못했습니다.");
            }
            Ok(Box::new(gui::App::default()))
        }),
    )
    .map_err(|err| anyhow::anyhow!("GUI 실행 실패: {}", err))
}

fn print_help() {
    println!("범용 사이트 수집기 v{}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("사용법:");
    println!("  site-collector detect <URL>      주소 구조 자동 분석");
    println!("  site-collector run [--days N]    등록된 사이트 수집");
    println!("  site-collector status            현재 상태 확인");
    println!("  site-collector commands          텔레그램 명령 처리");
    println!();
    println!("예시:");
    println!("  site-collector detect https://www.pcu.ac.kr/kor/article/Notify_1");
    println!("  site-collector run --days 2 --no-telegram");
}

/// 구조 분석
fn detect_command(args: &[String]) -> Result<()> {
    let Some(url) = args.get(2) else {
        println!("사용법: site-collector detect <URL>");
        return Ok(());
    };
    detect::validate_url(url)?;

    let cfg = config::Config::load();
    let fetcher = fetch::Fetcher::new(
        &cfg.crawl.user_agent,
        cfg.crawl.timeout_secs,
        cfg.crawl.delay_ms,
        cfg.crawl.use_js_engine,
    )?;

    println!("분석 중: {}", url);
    let result = detect::detect(&fetcher, url)?;
    println!("결과: {}", result.message);

    if result.ok {
        println!("링크 패턴: {}", result.structure.link_pattern);
        if !result.structure.detail_url_template.is_empty() {
            println!("상세 URL: {}", result.structure.detail_url_template);
        }
        println!("본문 선택자: {}", result.structure.body_selector);
        println!("페이지 패턴: {}", result.structure.page_pattern);
        println!("날짜 선택자: {}", result.structure.date_selector);
        println!();
        println!(
            "미리보기 ({}건 중 {}건 표시):",
            result.item_count,
            result.items.len()
        );
        for (i, item) in result.items.iter().enumerate() {
            let date = if item.date.is_empty() {
                "-".to_string()
            } else {
                item.date.clone()
            };
            println!("  {}. [{}] {}", i + 1, date, item.title);
            println!("     {}", item.url);
        }
    }
    Ok(())
}

/// 수집 실행
fn run_command(args: &[String]) -> Result<()> {
    let mut options = pipeline::RunOptions::default();
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--days" => {
                if let Some(v) = args.get(i + 1).and_then(|s| s.parse::<u32>().ok()) {
                    options.days = v.clamp(1, 30);
                    i += 1;
                }
            }
            "--no-telegram" => options.send_telegram = false,
            "--no-report" => options.make_report = false,
            "--include-seen" => options.include_seen = true,
            "--no-body" => options.fetch_body = false,
            other => println!("알 수 없는 옵션: {}", other),
        }
        i += 1;
    }

    let cfg = config::Config::load();
    if cfg.sites.is_empty() {
        println!("등록된 사이트가 없습니다. GUI 에서 사이트를 추가하거나 config.json 을 편집하세요.");
        return Ok(());
    }

    println!("수집 시작 (최근 {}일)", options.days);
    let log: pipeline::LogFn = Box::new(|msg: &str| println!("{}", msg));

    let result = pipeline::run(&cfg, &options, &log)?;

    println!();
    println!(
        "[완료] {} ~ {} / {}건 / 텔레그램 {}",
        result.from.format("%Y-%m-%d"),
        result.to.format("%Y-%m-%d"),
        result.posts.len(),
        if result.sent { "전송" } else { "미전송" }
    );
    if let Some(path) = &result.report_path {
        println!("[리포트] {}", path);
    }
    if !result.errors.is_empty() {
        println!("[오류]");
        for err in &result.errors {
            println!("  - {}", err);
        }
    }
    Ok(())
}

/// 상태 확인
fn status_command() -> Result<()> {
    let cfg = config::Config::load();
    let state = state::StateStore::load();
    let token = config::telegram_token();

    println!("=== 사이트 수집기 상태 ===");
    println!("설정 파일: {}", config::config_path().display());
    println!("등록 사이트: {}개", cfg.sites.len());
    for site in &cfg.sites {
        println!(
            "  {} {} ({})",
            if site.enabled { "🟢" } else { "⚪" },
            site.name,
            site.url
        );
    }
    println!();
    println!(
        "텔레그램: {}",
        if cfg.telegram.enabled { "켜짐" } else { "꺼짐" }
    );
    println!(
        "봇 토큰: {}",
        if token.is_empty() { "없음" } else { "설정됨" }
    );
    println!("chat_id: {}", cfg.telegram.chat_id);
    println!(
        "알림 일시정지: {}",
        if state.paused() { "예" } else { "아니오" }
    );
    println!("처리한 글: {}건", state.seen_count());
    println!();
    println!("=== 최근 실행 ===");
    let history = state.history();
    if history.is_empty() {
        println!("(기록 없음)");
    } else {
        for record in history.iter().rev().take(10) {
            println!(
                "  {} · {}건 · 텔레그램 {}",
                record.at,
                record.count,
                if record.sent { "전송" } else { "미전송" }
            );
        }
    }
    Ok(())
}

/// 텔레그램 명령 처리
fn commands_command() -> Result<()> {
    let mut cfg = config::Config::load();
    let token = config::telegram_token();
    if token.is_empty() {
        println!("텔레그램 봇 토큰이 없습니다.");
        return Ok(());
    }
    if !cfg.telegram_cmd.enabled {
        println!("명령 처리가 비활성화되어 있습니다.");
        return Ok(());
    }

    let offset = if cfg.telegram_cmd.offset > 0 {
        Some(cfg.telegram_cmd.offset)
    } else {
        None
    };

    let (commands, next_offset) = telegram::fetch_commands(&token, offset)?;
    if commands.is_empty() {
        cfg.telegram_cmd.offset = next_offset;
        cfg.save()?;
        println!("대기 중인 명령이 없습니다.");
        return Ok(());
    }

    let allowed = if cfg.telegram.chat_id.trim().is_empty() {
        None
    } else {
        Some(cfg.telegram.chat_id.trim().to_string())
    };

    let mut st = state::StateStore::load();
    let mut handled = 0;

    for (command, chat_id) in &commands {
        let chat_id_str = chat_id.to_string();
        if let Some(expected) = &allowed {
            if expected != &chat_id_str {
                println!("허용되지 않은 chat_id 무시: {}", chat_id_str);
                continue;
            }
        }
        println!("명령 처리: {}", command);
        handled += 1;

        let reply = build_reply(command, &cfg, &mut st)?;
        telegram::send_message(&token, &chat_id_str, &reply)?;
    }

    cfg.telegram_cmd.offset = next_offset;
    cfg.save()?;
    println!("{}개 명령 처리 완료", handled);
    Ok(())
}

/// 명령별 응답 메시지 생성
fn build_reply(command: &str, cfg: &config::Config, st: &mut state::StateStore) -> Result<String> {
    let reply = match command {
        "/help" | "/start" => telegram::help_text(),

        "/status" => {
            let mut text = String::from("<b>📊 수집기 상태</b>\n\n");
            text.push_str(if st.paused() {
                "⏸️ 알림 일시정지 중 (/resume 로 재개)\n\n"
            } else {
                "▶️ 알림 동작 중\n\n"
            });
            if let Some(record) = st.last_run() {
                text.push_str(&format!(
                    "마지막 실행: {}\n  · 수집 {}건\n\n",
                    record.at, record.count
                ));
            } else {
                text.push_str("마지막 실행: 기록 없음\n\n");
            }
            text.push_str("<b>등록 사이트</b>\n");
            if cfg.sites.is_empty() {
                text.push_str("  (없음)\n");
            } else {
                for site in &cfg.sites {
                    text.push_str(&format!(
                        "  {} {}\n",
                        if site.enabled { "🟢" } else { "⚪" },
                        telegram::escape_html(&site.name)
                    ));
                }
            }
            text.push_str(&format!("\n처리한 글: {}건", st.seen_count()));
            text
        }

        "/today" => {
            let runs = st.today_runs();
            if runs.is_empty() {
                "<b>📅 오늘 실행 기록</b>\n\n아직 실행 기록이 없습니다.".to_string()
            } else {
                let mut text = String::from("<b>📅 오늘 실행 기록</b>\n\n");
                let mut total = 0;
                for record in &runs {
                    let time = record.at.split(' ').nth(1).unwrap_or("-");
                    text.push_str(&format!(
                        "{} {}  {}건\n",
                        if record.sent { "✅" } else { "—" },
                        time,
                        record.count
                    ));
                    total += record.count;
                }
                text.push_str(&format!("\n<b>합계: {}건</b>", total));
                text
            }
        }

        "/pause" => {
            st.set_paused(true)?;
            "⏸️ 알림을 일시정지했습니다.\n/resume 으로 재개할 수 있습니다.".to_string()
        }

        "/resume" => {
            st.set_paused(false)?;
            "▶️ 알림을 재개했습니다.".to_string()
        }

        "/run" => {
            let log: pipeline::LogFn = Box::new(|msg: &str| println!("  {}", msg));
            let opts = pipeline::RunOptions {
                days: 2,
                ..Default::default()
            };
            match pipeline::run(cfg, &opts, &log) {
                Ok(result) => {
                    if result.posts.is_empty() {
                        "🔄 수집 완료 — 새 글이 없습니다.".to_string()
                    } else {
                        format!("🔄 수집 완료 — 새 글 {}건", result.posts.len())
                    }
                }
                Err(err) => format!("❌ 실행 중 오류: {}", err),
            }
        }

        other => format!(
            "❓ 알 수 없는 명령입니다: {}\n\n{}",
            telegram::escape_html(other),
            telegram::help_text()
        ),
    };
    Ok(reply)
}

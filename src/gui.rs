//! egui 기반 GUI
//!
//! 탭 구성: 수집 실행 / 사이트 관리 / 키워드 / 텔레그램 / 자동 실행 / 정보

use crate::config::{Config, KeywordMode, SiteSource};
use crate::detect;
use crate::fetch::Fetcher;
use crate::state::StateStore;
use eframe::egui;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// 백그라운드 작업이 GUI 로 보내는 메시지
enum WorkerMsg {
    Log(String),
    DetectDone(Box<detect::DetectResult>),
    RunDone(Box<crate::pipeline::RunResult>),
    Error(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Run,
    Sites,
    Telegram,
    Auto,
    Info,
}

pub struct App {
    config: Config,
    tab: Tab,
    logs: Vec<String>,

    // 사이트 추가/분석 입력
    input_url: String,
    input_name: String,
    input_keywords: String,
    detect_result: Option<detect::DetectResult>,
    busy: bool,

    // 실행 옵션
    run_days: u32,
    run_report: bool,
    run_telegram: bool,
    run_include_seen: bool,

    // 텔레그램 입력
    tg_token: String,
    tg_chat_id: String,
    tg_keyword_input: String,

    // 워커 통신
    tx: Sender<WorkerMsg>,
    rx: Option<Receiver<WorkerMsg>>,
    shared_logs: Arc<Mutex<Vec<String>>>,

    // 자동 실행
    auto_hour: u32,
    auto_minute: u32,
    auto_days: u32,
}

/// 작업 스케줄러 작업 이름
const TASK_NAME: &str = "SiteCollector_Daily";

impl Default for App {
    fn default() -> Self {
        let (tx, rx) = channel();
        let config = Config::load();
        Self {
            run_days: config.range_days.clamp(1, 30),
            tg_token: crate::config::telegram_token(),
            tg_chat_id: config.telegram.chat_id.clone(),
            tg_keyword_input: String::new(),
            config,
            tab: Tab::Run,
            logs: Vec::new(),
            input_url: String::new(),
            input_name: String::new(),
            input_keywords: String::new(),
            detect_result: None,
            busy: false,
            run_report: true,
            run_telegram: true,
            run_include_seen: false,
            tx,
            rx: Some(rx),
            shared_logs: Arc::new(Mutex::new(Vec::new())),
            auto_hour: 9,
            auto_minute: 0,
            auto_days: 2,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker();

        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("사이트 수집기");
                ui.separator();
                ui.selectable_value(&mut self.tab, Tab::Run, "수집 실행");
                ui.selectable_value(&mut self.tab, Tab::Sites, "사이트 관리");
                ui.selectable_value(&mut self.tab, Tab::Telegram, "텔레그램");
                ui.selectable_value(&mut self.tab, Tab::Auto, "자동 실행");
                ui.selectable_value(&mut self.tab, Tab::Info, "정보");
            });
        });

        egui::TopBottomPanel::bottom("log_panel")
            .resizable(true)
            .default_height(180.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("진행 상황");
                    if ui.small_button("지우기").clicked() {
                        self.logs.clear();
                    }
                });
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &self.logs {
                            ui.label(line);
                        }
                    });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.tab {
                Tab::Run => self.ui_run(ui),
                Tab::Sites => self.ui_sites(ui),
                Tab::Telegram => self.ui_telegram(ui),
                Tab::Auto => self.ui_auto(ui),
                Tab::Info => self.ui_info(ui),
            }
        });

        if self.busy {
            ctx.request_repaint();
        }
    }
}

impl App {
    // ---------------------------------------------------------- 워커 통신
    fn poll_worker(&mut self) {
        let Some(rx) = &self.rx else { return };
        let mut done = false;

        while let Ok(msg) = rx.try_recv() {
            match msg {
                WorkerMsg::Log(line) => self.logs.push(line),
                WorkerMsg::DetectDone(result) => {
                    self.logs.push(result.message.clone());
                    self.detect_result = Some(*result);
                    self.busy = false;
                    done = true;
                }
                WorkerMsg::RunDone(result) => {
                    self.logs.push(format!(
                        "[완료] {}건 / 텔레그램 {}",
                        result.posts.len(),
                        if result.sent { "전송" } else { "미전송" }
                    ));
                    self.busy = false;
                    done = true;
                }
                WorkerMsg::Error(err) => {
                    self.logs.push(format!("!! {}", err));
                    self.busy = false;
                    done = true;
                }
            }
        }
        let _ = done;
    }

    fn log_fn(&self) -> crate::pipeline::LogFn {
        let tx = self.tx.clone();
        Box::new(move |msg: &str| {
            let _ = tx.send(WorkerMsg::Log(msg.to_string()));
        })
    }

    // ------------------------------------------------------------ 수집 실행 탭
    fn ui_run(&mut self, ui: &mut egui::Ui) {
        ui.heading("수집 실행");
        ui.add_space(6.0);

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("최근");
                ui.add(egui::DragValue::new(&mut self.run_days).range(1..=30).speed(0.2));
                ui.label("일");
            });
            ui.checkbox(&mut self.run_report, "리포트(Markdown) 생성");
            ui.checkbox(&mut self.run_telegram, "텔레그램 전송");
            ui.checkbox(&mut self.run_include_seen, "이미 처리한 글도 포함");
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let enabled = !self.busy && !self.config.sites.is_empty();
            if ui
                .add_enabled(enabled, egui::Button::new("▶ 수집 시작"))
                .clicked()
            {
                self.start_run();
            }
            if self.busy {
                ui.spinner();
                ui.label("작업 중...");
            }
            if self.config.sites.is_empty() {
                ui.colored_label(egui::Color32::from_rgb(200, 120, 0), "등록된 사이트가 없습니다");
            }
        });

        ui.add_space(10.0);
        ui.separator();
        ui.heading("등록된 사이트");
        ui.add_space(4.0);

        if self.config.sites.is_empty() {
            ui.label("'사이트 관리' 탭에서 주소를 추가하세요.");
        } else {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for site in &self.config.sites {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.strong(&site.name);
                            if site.needs_js {
                                ui.colored_label(
                                    egui::Color32::from_rgb(200, 150, 0),
                                    "(JS 필요)",
                                );
                            }
                        });
                        ui.label(&site.url);
                        if !site.keywords.is_empty() {
                            ui.label(format!("키워드: {}", site.keywords.join(", ")));
                        }
                        ui.label(format!("감지: {}", site.detect_message));
                    });
                }
            });
        }
    }

    fn start_run(&mut self) {
        self.busy = true;
        self.logs.clear();
        self.logs.push("=== 수집 시작 ===".to_string());

        let config = self.config.clone();
        let options = crate::pipeline::RunOptions {
            days: self.run_days,
            make_report: self.run_report,
            send_telegram: self.run_telegram,
            include_seen: self.run_include_seen,
            fetch_body: true,
        };
        let tx = self.tx.clone();

        std::thread::spawn(move || {
            let log_tx = tx.clone();
            let log: crate::pipeline::LogFn = Box::new(move |msg: &str| {
                let _ = log_tx.send(WorkerMsg::Log(msg.to_string()));
            });
            match crate::pipeline::run(&config, &options, &log) {
                Ok(result) => {
                    let _ = tx.send(WorkerMsg::RunDone(Box::new(result)));
                }
                Err(err) => {
                    let _ = tx.send(WorkerMsg::Error(err.to_string()));
                }
            }
        });
    }

    // ---------------------------------------------------------- 사이트 관리 탭
    fn ui_sites(&mut self, ui: &mut egui::Ui) {
        ui.heading("사이트 관리");
        ui.label("주소를 넣고 '자동 분석'을 누르면 게시판 구조를 감지합니다.");
        ui.add_space(8.0);

        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::Grid::new("site_input")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label("주소");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.input_url)
                            .hint_text("https://example.com/notice")
                            .desired_width(420.0),
                    );
                    ui.end_row();

                    ui.label("이름");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.input_name)
                            .hint_text("예시 게시판")
                            .desired_width(260.0),
                    );
                    ui.end_row();

                    ui.label("키워드");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.input_keywords)
                            .hint_text("자격증, 공모전 (쉼표 구분, 비우면 전체)")
                            .desired_width(420.0),
                    );
                    ui.end_row();
                });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let can_detect = !self.busy && !self.input_url.trim().is_empty();
                if ui
                    .add_enabled(can_detect, egui::Button::new("🔍 자동 분석"))
                    .clicked()
                {
                    self.start_detect();
                }

                let can_add = self.detect_result.is_some() && !self.busy;
                if ui
                    .add_enabled(can_add, egui::Button::new("➕ 추가"))
                    .clicked()
                {
                    self.add_detected_site();
                }

                if self.busy {
                    ui.spinner();
                }
            });
        });

        self.ui_detect_result(ui);

        ui.add_space(12.0);
        ui.separator();
        ui.heading(format!("등록된 사이트 ({})", self.config.sites.len()));
        ui.add_space(4.0);

        let mut to_remove: Option<usize> = None;
        let mut changed = false;

        for (index, site) in self.config.sites.iter_mut().enumerate() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut site.enabled, "").changed() {
                        changed = true;
                    }
                    ui.strong(&site.name);
                    ui.label(&site.url);
                    if ui.small_button("삭제").clicked() {
                        to_remove = Some(index);
                    }
                });
                if !site.keywords.is_empty() {
                    ui.label(format!("키워드: {}", site.keywords.join(", ")));
                }
                ui.label(format!("감지: {}", site.detect_message));
            });
        }

        if let Some(index) = to_remove {
            self.config.sites.remove(index);
            let _ = self.config.save();
            self.logs.push("사이트를 삭제했습니다.".to_string());
        } else if changed {
            let _ = self.config.save();
        }
    }

    fn ui_detect_result(&mut self, ui: &mut egui::Ui) {
        let Some(result) = &self.detect_result else {
            return;
        };
        ui.add_space(10.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.heading("분석 결과");
            let color = if result.ok {
                egui::Color32::from_rgb(40, 150, 60)
            } else {
                egui::Color32::from_rgb(200, 60, 60)
            };
            ui.colored_label(color, &result.message);
            ui.label(format!("링크 패턴: {}", result.structure.link_pattern));
            if !result.structure.detail_url_template.is_empty() {
                ui.label(format!("상세 URL: {}", result.structure.detail_url_template));
            }
            ui.label(format!("본문 선택자: {}", result.structure.body_selector));
            ui.label(format!("페이지 패턴: {}", result.structure.page_pattern));

            if !result.items.is_empty() {
                ui.add_space(6.0);
                ui.strong("미리보기");
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for (i, item) in result.items.iter().enumerate() {
                            let date = if item.date.is_empty() {
                                "-".to_string()
                            } else {
                                item.date.clone()
                            };
                            ui.label(format!("{}. [{}] {}", i + 1, date, item.title));
                        }
                    });
            }
        });
    }

    fn start_detect(&mut self) {
        let url = self.input_url.trim().to_string();
        if url.is_empty() {
            return;
        }
        if let Err(err) = detect::validate_url(&url) {
            self.logs.push(format!("!! {}", err));
            return;
        }

        self.busy = true;
        self.detect_result = None;
        self.logs.push(format!("분석 중: {}", url));

        let config = self.config.clone();
        let tx = self.tx.clone();

        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<detect::DetectResult> {
                let fetcher = Fetcher::new(
                    &config.crawl.user_agent,
                    config.crawl.timeout_secs,
                    config.crawl.delay_ms,
                    config.crawl.use_js_engine,
                )?;
                detect::detect(&fetcher, &url)
            })();

            match result {
                Ok(res) => {
                    let _ = tx.send(WorkerMsg::DetectDone(Box::new(res)));
                }
                Err(err) => {
                    let _ = tx.send(WorkerMsg::Error(err.to_string()));
                }
            }
        });
    }

    fn add_detected_site(&mut self) {
        let Some(result) = self.detect_result.take() else {
            return;
        };
        if !result.ok {
            self.logs
                .push("!! 감지에 실패한 사이트는 추가할 수 없습니다.".to_string());
            return;
        }

        let url = self.input_url.trim().to_string();
        let name = if self.input_name.trim().is_empty() {
            url::Url::parse(&url)
                .ok()
                .and_then(|u| u.host_str().map(|h| h.to_string()))
                .unwrap_or_else(|| "새 사이트".to_string())
        } else {
            self.input_name.trim().to_string()
        };

        let keywords: Vec<String> = self
            .input_keywords
            .split(',')
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .collect();

        let site = SiteSource {
            id: self.config.next_site_id(),
            name: name.clone(),
            url,
            keywords,
            enabled: true,
            needs_js: result.needs_js,
            structure: result.structure,
            detected_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            detect_message: result.message.clone(),
        };

        self.config.sites.push(site);
        if let Err(err) = self.config.save() {
            self.logs.push(format!("!! 설정 저장 실패: {}", err));
        } else {
            self.logs.push(format!("사이트 추가됨: {}", name));
        }

        self.input_url.clear();
        self.input_name.clear();
        self.input_keywords.clear();
    }

    // ---------------------------------------------------------- 텔레그램 탭
    fn ui_telegram(&mut self, ui: &mut egui::Ui) {
        ui.heading("텔레그램");
        ui.label("BotFather(@BotFather) 로 봇을 만들고 토큰을 붙여넣으세요.");
        ui.add_space(8.0);

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.checkbox(&mut self.config.telegram.enabled, "텔레그램 알림 사용");

            egui::Grid::new("tg_input")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label("봇 토큰");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.tg_token)
                            .password(true)
                            .hint_text("123456789:AAH...")
                            .desired_width(400.0),
                    );
                    ui.end_row();

                    ui.label("chat_id");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.tg_chat_id)
                            .hint_text("자동 찾기를 누르면 채워집니다")
                            .desired_width(200.0),
                    );
                    ui.end_row();
                });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("자동 찾기").clicked() {
                    self.find_chat_id();
                }
                if ui.button("테스트 메시지").clicked() {
                    self.test_telegram();
                }
                if ui.button("저장").clicked() {
                    self.save_telegram();
                }
            });
        });

        ui.add_space(10.0);
        ui.separator();
        ui.heading("알림 키워드 조건");
        ui.add_space(4.0);
        self.ui_telegram_keywords(ui);

        ui.add_space(10.0);
        ui.label("명령어: /status  /today  /run  /pause  /resume  /help");
    }

    fn ui_telegram_keywords(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            for mode in KeywordMode::all() {
                if ui
                    .radio(self.config.telegram.keyword_mode == mode, mode.label())
                    .clicked()
                {
                    self.config.telegram.keyword_mode = mode;
                    let _ = self.config.save();
                }
                ui.label(egui::RichText::new(mode.description()).small().weak());
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.tg_keyword_input)
                        .hint_text("키워드 입력 후 추가")
                        .desired_width(200.0),
                );
                if ui.button("추가").clicked() {
                    let kw = self.tg_keyword_input.trim().to_string();
                    if !kw.is_empty() && !self.config.telegram.notify_keywords.contains(&kw) {
                        self.config.telegram.notify_keywords.push(kw);
                        let _ = self.config.save();
                    }
                    self.tg_keyword_input.clear();
                }
                if ui.button("전체 비우기").clicked() {
                    self.config.telegram.notify_keywords.clear();
                    let _ = self.config.save();
                }
            });

            if self.config.telegram.notify_keywords.is_empty() {
                ui.weak("등록된 키워드 없음 (전체 알림)");
            } else {
                let mut removed: Option<usize> = None;
                ui.horizontal_wrapped(|ui| {
                    for (i, kw) in self.config.telegram.notify_keywords.iter().enumerate() {
                        if ui.button(format!("{} ✕", kw)).clicked() {
                            removed = Some(i);
                        }
                    }
                });
                if let Some(i) = removed {
                    self.config.telegram.notify_keywords.remove(i);
                    let _ = self.config.save();
                }
            }
        });
    }

    fn save_telegram(&mut self) {
        if !self.tg_token.trim().is_empty() {
            if let Err(err) = crate::config::write_env("TELEGRAM_BOT_TOKEN", self.tg_token.trim()) {
                self.logs.push(format!("!! 토큰 저장 실패: {}", err));
            }
        }
        self.config.telegram.chat_id = self.tg_chat_id.trim().to_string();
        if let Err(err) = self.config.save() {
            self.logs.push(format!("!! 설정 저장 실패: {}", err));
        } else {
            self.logs.push("텔레그램 설정을 저장했습니다.".to_string());
        }
    }

    fn find_chat_id(&mut self) {
        let token = self.tg_token.trim().to_string();
        if token.is_empty() {
            self.logs.push("!! 봇 토큰을 먼저 입력하세요.".to_string());
            return;
        }

        match crate::telegram::get_me(&token) {
            Ok((username, name)) => self.logs.push(format!("봇 확인: @{} ({})", username, name)),
            Err(err) => {
                self.logs.push(format!("!! 봇 확인 실패: {}", err));
                return;
            }
        }

        match crate::telegram::get_chat_ids(&token) {
            Ok(chats) if chats.is_empty() => self.logs.push(
                "chat_id 를 찾지 못했습니다. 봇에게 /start 를 보낸 뒤 다시 시도하세요.".to_string(),
            ),
            Ok(chats) => {
                let (id, name, kind) = chats.last().unwrap().clone();
                self.tg_chat_id = id.to_string();
                self.config.telegram.chat_id = id.to_string();
                self.logs
                    .push(format!("chat_id 발견: {} ({}, {})", id, name, kind));
                self.save_telegram();
            }
            Err(err) => self.logs.push(format!("!! chat_id 조회 실패: {}", err)),
        }
    }

    fn test_telegram(&mut self) {
        let token = self.tg_token.trim().to_string();
        let chat_id = self.tg_chat_id.trim().to_string();
        if token.is_empty() || chat_id.is_empty() {
            self.logs.push("!! 토큰과 chat_id 를 입력하세요.".to_string());
            return;
        }
        match crate::telegram::send_message(
            &token,
            &chat_id,
            "✅ 테스트 메시지입니다. 사이트 수집기 알림이 정상 동작합니다.",
        ) {
            Ok(()) => self.logs.push("테스트 메시지 전송 완료".to_string()),
            Err(err) => self.logs.push(format!("!! 전송 실패: {}", err)),
        }
    }

    // ---------------------------------------------------------- 자동 실행 탭
    fn ui_auto(&mut self, ui: &mut egui::Ui) {
        ui.heading("자동 실행");
        ui.label("Windows 작업 스케줄러에 등록하면 정해진 시각에 자동으로 수집합니다.");
        ui.add_space(8.0);

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("매일");
                ui.add(egui::DragValue::new(&mut self.auto_hour).range(0..=23).speed(0.2));
                ui.label("시");
                ui.add(egui::DragValue::new(&mut self.auto_minute).range(0..=59).speed(0.5));
                ui.label("분");
            });
            ui.horizontal(|ui| {
                ui.label("수집 범위");
                ui.add(egui::DragValue::new(&mut self.auto_days).range(1..=30).speed(0.2));
                ui.label("일");
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("작업 스케줄러에 등록").clicked() {
                    self.register_task();
                }
                if ui.button("등록 해제").clicked() {
                    self.unregister_task();
                }
                if ui.button("등록 상태 확인").clicked() {
                    self.check_task();
                }
            });
        });

        ui.add_space(10.0);
        ui.separator();
        ui.heading("최근 실행 기록");
        ui.add_space(4.0);

        let state = StateStore::load();
        let history = state.history();
        if history.is_empty() {
            ui.label("아직 실행 기록이 없습니다.");
        } else {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for record in history.iter().rev().take(30) {
                    ui.label(format!(
                        "{} · {}건 · 텔레그램 {}",
                        record.at,
                        record.count,
                        if record.sent { "전송" } else { "미전송" }
                    ));
                }
            });
        }
    }

    fn register_task(&mut self) {
        let exe = std::env::current_exe().unwrap_or_default();
        let time = format!("{:02}:{:02}", self.auto_hour, self.auto_minute);
        let command = format!("\"{}\" run --days {}", exe.display(), self.auto_days);

        let output = std::process::Command::new("schtasks")
            .args([
                "/Create", "/F", "/SC", "DAILY", "/TN", TASK_NAME, "/TR", &command, "/ST",
                &time,
            ])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                self.logs
                    .push(format!("작업 스케줄러 등록 완료: 매일 {}", time));
            }
            Ok(out) => {
                let err = decode_console(&out.stderr);
                self.logs.push(format!("!! 등록 실패: {}", err.trim()));
            }
            Err(err) => self.logs.push(format!("!! 등록 실패: {}", err)),
        }
    }

    fn unregister_task(&mut self) {
        let output = std::process::Command::new("schtasks")
            .args(["/Delete", "/F", "/TN", TASK_NAME])
            .output();
        match output {
            Ok(out) if out.status.success() => {
                self.logs.push("작업 스케줄러 등록을 해제했습니다.".to_string());
            }
            Ok(_) => self.logs.push("등록된 작업이 없습니다.".to_string()),
            Err(err) => self.logs.push(format!("!! 해제 실패: {}", err)),
        }
    }

    fn check_task(&mut self) {
        let output = std::process::Command::new("schtasks")
            .args(["/Query", "/TN", TASK_NAME, "/FO", "LIST"])
            .output();
        match output {
            Ok(out) if out.status.success() => {
                let text = decode_console(&out.stdout);
                for line in text.lines() {
                    let t = line.trim();
                    if t.contains("다음 실행")
                        || t.contains("Next Run")
                        || t.contains("Status")
                        || t.contains("TaskName")
                    {
                        self.logs.push(t.to_string());
                    }
                }
            }
            Ok(_) => self.logs.push("등록된 작업이 없습니다.".to_string()),
            Err(err) => self.logs.push(format!("!! 확인 실패: {}", err)),
        }
    }

    // -------------------------------------------------------------- 정보 탭
    fn ui_info(&mut self, ui: &mut egui::Ui) {
        ui.heading("정보");
        ui.add_space(6.0);

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(format!("버전: {}", env!("CARGO_PKG_VERSION")));
            ui.label(format!("프로그램 폴더: {}", crate::config::app_dir().display()));
            ui.label(format!("설정 파일: {}", crate::config::config_path().display()));
            ui.label(format!(
                "리포트 폴더: {}",
                crate::config::output_dir(&self.config).display()
            ));
            let font_ok = crate::font::installed_font().is_some();
            let color = if font_ok {
                egui::Color32::from_rgb(40, 150, 60)
            } else {
                egui::Color32::from_rgb(200, 60, 60)
            };
            ui.colored_label(color, crate::font::font_status());
        });

        ui.add_space(10.0);
        ui.heading("JavaScript 엔진 (선택)");
        ui.label("React/Vue 같은 동적 사이트는 옵션 엔진이 필요합니다.");
        let engine = crate::js_engine::JsEngine::detect();
        if engine.available {
            ui.colored_label(egui::Color32::from_rgb(40, 150, 60), &engine.detail);
        } else {
            ui.colored_label(egui::Color32::from_rgb(200, 120, 0), &engine.detail);
            ui.add_space(4.0);
            ui.label("설치 방법 (둘 중 하나):");
            ui.monospace("1) pip install playwright && playwright install chromium");
            ui.monospace("2) npm install -g puppeteer");
        }

        ui.add_space(10.0);
        ui.heading("사용법");
        ui.label("1. '사이트 관리' 탭에서 주소를 입력하고 '자동 분석'을 누릅니다.");
        ui.label("2. 감지 결과를 확인하고 '추가'를 누릅니다.");
        ui.label("3. '수집 실행' 탭에서 '수집 시작'을 누르면 리포트가 생성됩니다.");
        ui.label("4. '자동 실행' 탭에서 매일 자동 수집을 등록할 수 있습니다.");
    }
}

/// Windows 콘솔 출력 인코딩 처리
fn decode_console(bytes: &[u8]) -> String {
    std::str::from_utf8(bytes)
        .map(|s| s.to_string())
        .unwrap_or_else(|_| String::from_utf8_lossy(bytes).to_string())
}

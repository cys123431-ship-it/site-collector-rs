//! 설정 관리
//!
//! config.json 을 읽고 쓴다. 개인 정보(chat_id, 토큰)는 별도 파일(.env)에 둔다.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// 앱 루트 디렉터리 (exe 가 있는 폴더, 개발 중이면 프로젝트 루트)
pub fn app_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            // target/debug 나 target/release 안이면 두 단계 위로
            if dir.ends_with("debug") || dir.ends_with("release") {
                if let Some(root) = dir.parent().and_then(|p| p.parent()) {
                    return root.to_path_buf();
                }
            }
            return dir.to_path_buf();
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn config_path() -> PathBuf {
    app_dir().join("config.json")
}

pub fn env_path() -> PathBuf {
    app_dir().join(".env")
}

pub fn data_dir() -> PathBuf {
    let dir = app_dir().join("data");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn logs_dir() -> PathBuf {
    let dir = app_dir().join("logs");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn output_dir(config: &Config) -> PathBuf {
    if config.output.dir.trim().is_empty() {
        let dir = app_dir().join("output");
        let _ = fs::create_dir_all(&dir);
        dir
    } else {
        let dir = PathBuf::from(&config.output.dir);
        let _ = fs::create_dir_all(&dir);
        dir
    }
}

/// 키워드 필터 모드
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum KeywordMode {
    /// 기본: 수집된 글 전부
    #[default]
    All,
    /// 내가 추가한 키워드만
    KeywordOnly,
    /// 기본 + 내 키워드 (OR)
    DefaultOrKeyword,
    /// 기본 + 내 키워드 (AND)
    DefaultAndKeyword,
}

impl KeywordMode {
    pub fn label(&self) -> &'static str {
        match self {
            KeywordMode::All => "전부",
            KeywordMode::KeywordOnly => "키워드 단독",
            KeywordMode::DefaultOrKeyword => "OR 조건",
            KeywordMode::DefaultAndKeyword => "AND 조건",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            KeywordMode::All => "수집된 글을 모두 알립니다. (추가 키워드 무시)",
            KeywordMode::KeywordOnly => "아래 키워드에 걸린 글만 알립니다.",
            KeywordMode::DefaultOrKeyword => "기본 키워드 또는 아래 키워드에 걸린 글 (범위 넓음)",
            KeywordMode::DefaultAndKeyword => "기본 키워드이면서 아래 키워드에도 걸린 글 (범위 좁음)",
        }
    }

    pub fn all() -> [KeywordMode; 4] {
        [
            KeywordMode::All,
            KeywordMode::KeywordOnly,
            KeywordMode::DefaultOrKeyword,
            KeywordMode::DefaultAndKeyword,
        ]
    }
}

/// 게시글 링크/제목/날짜/본문 위치 (자동 감지 결과)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DetectedStructure {
    /// 링크 패턴 (예: "/notice/{id}")
    pub link_pattern: String,
    /// 목록 항목 컨테이너 CSS 선택자
    pub item_selector: String,
    /// 제목 CSS 선택자
    pub title_selector: String,
    /// 날짜 CSS 선택자
    pub date_selector: String,
    /// 본문 CSS 선택자 (상세 페이지)
    pub body_selector: String,
    /// 페이지 이동 패턴 (예: "?page={n}")
    pub page_pattern: String,
    /// 상세 페이지 URL 템플릿 (예: "{base}/{id}") — onclick 게시글용
    #[serde(default)]
    pub detail_url_template: String,
    /// 상세 페이지에서 본문 발견 여부
    pub body_detected: bool,
}

/// 사용자가 추가한 수집 사이트
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteSource {
    pub id: String,
    pub name: String,
    pub url: String,
    pub keywords: Vec<String>,
    pub enabled: bool,
    /// JavaScript 렌더링 필요 여부
    pub needs_js: bool,
    pub structure: DetectedStructure,
    pub detected_at: String,
    pub detect_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    #[serde(default)]
    pub dir: String,
    #[serde(default = "default_true")]
    pub include_body: bool,
    #[serde(default = "default_body_max")]
    pub body_max_chars: usize,
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_true() -> bool {
    true
}
fn default_body_max() -> usize {
    4000
}
fn default_format() -> String {
    "md".to_string()
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            dir: String::new(),
            include_body: true,
            body_max_chars: 4000,
            format: default_format(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub chat_id: String,
    #[serde(default = "default_prefix")]
    pub header_prefix: String,
    #[serde(default = "default_interval")]
    pub interval_hours: u32,
    #[serde(default)]
    pub notify_keywords: Vec<String>,
    #[serde(default)]
    pub keyword_mode: KeywordMode,
}

fn default_prefix() -> String {
    "[사이트 수집]".to_string()
}
fn default_interval() -> u32 {
    2
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            chat_id: String::new(),
            header_prefix: default_prefix(),
            interval_hours: default_interval(),
            notify_keywords: Vec::new(),
            keyword_mode: KeywordMode::All,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramCmdConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub offset: i64,
}

impl Default for TelegramCmdConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            offset: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundConfig {
    #[serde(default)]
    pub tray_enabled: bool,
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,
    #[serde(default = "default_true")]
    pub start_hidden: bool,
}

impl Default for BackgroundConfig {
    fn default() -> Self {
        Self {
            tray_enabled: false,
            minimize_to_tray: true,
            start_hidden: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlConfig {
    #[serde(default = "default_pages")]
    pub max_pages: u32,
    #[serde(default = "default_delay")]
    pub delay_ms: u64,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_ua")]
    pub user_agent: String,
    /// JavaScript 렌더링 사용 (Playwright 설치 시)
    #[serde(default)]
    pub use_js_engine: bool,
}

fn default_pages() -> u32 {
    3
}
fn default_delay() -> u64 {
    400
}
fn default_timeout() -> u64 {
    20
}
fn default_ua() -> String {
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36".to_string()
}

impl Default for CrawlConfig {
    fn default() -> Self {
        Self {
            max_pages: default_pages(),
            delay_ms: default_delay(),
            timeout_secs: default_timeout(),
            user_agent: default_ua(),
            use_js_engine: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StabilityConfig {
    #[serde(default = "default_true")]
    pub log_to_file: bool,
    #[serde(default = "default_threshold")]
    pub failure_alert_threshold: u32,
}

fn default_threshold() -> u32 {
    3
}

impl Default for StabilityConfig {
    fn default() -> Self {
        Self {
            log_to_file: true,
            failure_alert_threshold: default_threshold(),
        }
    }
}

fn default_days() -> u32 {
    2
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub sites: Vec<SiteSource>,
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default)]
    pub telegram: TelegramConfig,
    #[serde(default)]
    pub telegram_cmd: TelegramCmdConfig,
    #[serde(default)]
    pub background: BackgroundConfig,
    #[serde(default)]
    pub crawl: CrawlConfig,
    #[serde(default)]
    pub stability: StabilityConfig,
    /// 수집 기간 (최근 N일)
    #[serde(default = "default_days")]
    pub range_days: u32,
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        if !path.exists() {
            return Config::default();
        }
        match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Config::default(),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let text = serde_json::to_string_pretty(self)?;
        fs::write(config_path(), text)?;
        Ok(())
    }

    /// 다음 사이트 id 생성
    pub fn next_site_id(&self) -> String {
        let mut n = self.sites.len() + 1;
        loop {
            let id = format!("site_{}", n);
            if !self.sites.iter().any(|s| s.id == id) {
                return id;
            }
            n += 1;
        }
    }
}

/// 기본 키워드 (AND/OR 모드에서 '기본 키워드' 판정에 사용)
pub fn default_keywords() -> Vec<String> {
    [
        "공지", "안내", "모집", "신청", "공모", "공모전", "경진대회", "해커톤",
        "자격증", "시험", "접수", "장학", "취업", "인턴", "채용", "교육",
        "프로그램", "행사", "설명회", "특강", "세미나",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// .env 파일에서 값 읽기
pub fn read_env(key: &str) -> String {
    let Ok(text) = fs::read_to_string(env_path()) else {
        return String::new();
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix(key) {
            if let Some(value) = rest.strip_prefix('=') {
                return value.trim().trim_matches('"').trim_matches('\'').to_string();
            }
        }
    }
    String::new()
}

/// .env 파일에 값 저장 (다른 키는 유지)
pub fn write_env(key: &str, value: &str) -> anyhow::Result<()> {
    let path = env_path();
    let mut lines: Vec<String> = Vec::new();
    let mut found = false;

    if let Ok(text) = fs::read_to_string(&path) {
        for line in text.lines() {
            if line.trim_start().starts_with(&format!("{}=", key)) {
                lines.push(format!("{}={}", key, value));
                found = true;
            } else {
                lines.push(line.to_string());
            }
        }
    }
    if !found {
        lines.push(format!("{}={}", key, value));
    }
    fs::write(&path, lines.join("\n") + "\n")?;
    Ok(())
}

/// 텔레그램 봇 토큰 (환경변수 우선)
pub fn telegram_token() -> String {
    let env = std::env::var("TELEGRAM_BOT_TOKEN").unwrap_or_default();
    if !env.trim().is_empty() {
        return env;
    }
    read_env("TELEGRAM_BOT_TOKEN")
}

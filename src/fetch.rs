//! HTTP 수집
//!
//! 기본은 requests 역할의 가벼운 HTTP.
//! JavaScript 렌더링이 필요한 사이트는 옵션으로 설치한 브라우저 엔진을 호출한다.

use anyhow::{anyhow, Result};
use std::time::Duration;

pub struct Fetcher {
    client: reqwest::blocking::Client,
    use_js: bool,
    delay_ms: u64,
}

impl Fetcher {
    pub fn new(user_agent: &str, timeout_secs: u64, delay_ms: u64, use_js: bool) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(user_agent)
            .timeout(Duration::from_secs(timeout_secs))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()?;
        Ok(Self {
            client,
            use_js,
            delay_ms,
        })
    }

    /// 페이지 HTML 가져오기.
    /// JS 엔진이 켜져 있으면 브라우저로, 아니면 정적 HTTP 로 가져온다.
    pub fn get(&self, url: &str) -> Result<String> {
        if self.delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(self.delay_ms));
        }

        if self.use_js {
            match fetch_with_browser(url) {
                Ok(html) => return Ok(html),
                Err(err) => {
                    tracing::warn!("브라우저 엔진 실패, 정적 수집으로 대체: {err}");
                }
            }
        }
        self.get_static(url)
    }

    /// 정적 HTML 만 가져온다 (JS 미실행)
    pub fn get_static(&self, url: &str) -> Result<String> {
        let resp = self.client.get(url).send()?;
        let status = resp.status();
        if !status.is_success() {
            return Err(anyhow!("HTTP {} - {}", status.as_u16(), url));
        }
        let text = resp.text()?;
        Ok(text)
    }

    /// JS 필요 여부를 판단한다.
    /// 정적 HTML 에 링크/텍스트가 거의 없으면 JS 렌더링 사이트로 본다.
    pub fn looks_like_spa(html: &str) -> bool {
        let lower = html.to_lowercase();
        // 대표적인 SPA 루트 마커
        let markers = [
            "<div id=\"root\"></div>",
            "<div id=\"app\"></div>",
            "<div id=\"__next\"></div>",
            "<app-root>",
        ];
        if markers.iter().any(|m| lower.contains(m)) {
            return true;
        }
        // 본문 텍스트가 지나치게 적으면 JS 렌더링 의심
        let visible = strip_tags_len(html);
        visible < 500 && lower.contains("<script")
    }
}

fn strip_tags_len(html: &str) -> usize {
    let mut out = String::with_capacity(html.len() / 2);
    let mut in_tag = false;
    let mut in_script = false;
    let lower = html.to_lowercase();
    let bytes = html.as_bytes();

    // 아주 단순한 태그 제거 (정확도보다 속도)
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        if !in_tag && ch == '<' {
            if lower[i..].starts_with("<script") || lower[i..].starts_with("<style") {
                in_script = true;
            }
            in_tag = true;
        } else if in_tag && ch == '>' {
            if lower[..i + 1].ends_with("</script>") || lower[..i + 1].ends_with("</style>") {
                in_script = false;
            }
            in_tag = false;
        } else if !in_tag && !in_script {
            out.push(ch);
        }
        i += 1;
    }
    out.split_whitespace().map(|w| w.len() + 1).sum()
}

/// 옵션 브라우저 엔진 (Playwright 등) 호출
///
/// 이 프로젝트는 경량 배포를 우선하므로 브라우저 엔진을 기본 포함하지 않는다.
/// 사용자가 옵션을 설치했을 때만 동작한다.
fn fetch_with_browser(url: &str) -> Result<String> {
    let engine = crate::js_engine::JsEngine::detect();
    if !engine.available {
        return Err(anyhow!(
            "JavaScript 엔진이 설치되어 있지 않습니다. 옵션 설치가 필요합니다."
        ));
    }
    engine.fetch(url)
}

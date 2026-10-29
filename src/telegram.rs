//! 텔레그램 알림 및 명령어
//!
//! BotFather 로 만든 봇 토큰만 있으면 동작한다.
//! 외부 서버 없이 getUpdates 폴링 방식으로 명령을 받는다.

use crate::filter::Post;
use anyhow::{anyhow, Result};
use serde_json::Value;

const API: &str = "https://api.telegram.org/bot";

/// HTML 이스케이프 (parse_mode=HTML)
pub fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// API 호출
fn call(token: &str, method: &str, params: &[(&str, String)]) -> Result<Value> {
    let url = format!("{}{}/{}", API, token, method);
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?;

    let resp = if params.is_empty() {
        client.get(&url).send()?
    } else {
        client.post(&url).form(params).send()?
    };

    let status = resp.status();
    let text = resp.text()?;
    let preview: String = text.chars().take(180).collect();
    let value: Value = serde_json::from_str(&text)
        .map_err(|_| anyhow!("응답 파싱 실패 (HTTP {}): {}", status.as_u16(), preview))?;

    if value.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        let desc = value
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("알 수 없는 오류");
        return Err(anyhow!("텔레그램 API 오류: {}", desc));
    }
    Ok(value)
}

/// 봇 정보 확인 (토큰 검증)
pub fn get_me(token: &str) -> Result<(String, String)> {
    let value = call(token, "getMe", &[])?;
    let result = value.get("result").cloned().unwrap_or(Value::Null);
    let username = result
        .get("username")
        .and_then(|v| v.as_str())
        .unwrap_or("?")
        .to_string();
    let name = result
        .get("first_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok((username, name))
}

/// 최근 메시지에서 chat_id 목록을 얻는다. (id, 이름, 종류)
pub fn get_chat_ids(token: &str) -> Result<Vec<(i64, String, String)>> {
    let value = call(
        token,
        "getUpdates",
        &[("limit", "100".to_string()), ("offset", "-100".to_string())],
    )?;
    let mut chats: Vec<(i64, String, String)> = Vec::new();

    if let Some(results) = value.get("result").and_then(|v| v.as_array()) {
        for update in results {
            let message = update
                .get("message")
                .or_else(|| update.get("channel_post"));
            let Some(message) = message else { continue };
            let Some(chat) = message.get("chat") else { continue };
            let Some(id) = chat.get("id").and_then(|v| v.as_i64()) else {
                continue;
            };
            if chats.iter().any(|(existing, _, _)| *existing == id) {
                continue;
            }
            let name = chat
                .get("title")
                .and_then(|v| v.as_str())
                .or_else(|| chat.get("first_name").and_then(|v| v.as_str()))
                .or_else(|| chat.get("username").and_then(|v| v.as_str()))
                .unwrap_or("")
                .to_string();
            let kind = chat
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            chats.push((id, name, kind));
        }
    }
    Ok(chats)
}

/// 메시지 전송
pub fn send_message(token: &str, chat_id: &str, text: &str) -> Result<()> {
    for chunk in split_message(text) {
        call(
            token,
            "sendMessage",
            &[
                ("chat_id", chat_id.to_string()),
                ("text", chunk),
                ("parse_mode", "HTML".to_string()),
                ("disable_web_page_preview", "true".to_string()),
            ],
        )?;
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    Ok(())
}

/// 4096자 제한에 맞춰 분할
fn split_message(text: &str) -> Vec<String> {
    const LIMIT: usize = 3900;
    if text.chars().count() <= LIMIT {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        if current.chars().count() + line.chars().count() + 1 > LIMIT && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// 게시글 목록을 메시지로 만든다. (제목 + 링크만)
pub fn format_posts(posts: &[Post], header: &str) -> String {
    if posts.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str(&format!("<b>{}</b>\n\n", escape_html(header)));

    for post in posts {
        out.push_str(&format!(
            "🔵 <a href=\"{}\">{}</a>\n",
            escape_html(&post.url),
            escape_html(&post.title)
        ));
        out.push_str(&format!(
            "　　{} · {}\n\n",
            escape_html(&post.site_name),
            post.date_label()
        ));
    }
    out.push_str(&format!("<b>총 {}건</b>", posts.len()));
    out
}

/// 수집 결과를 전송한다.
pub fn send_posts(token: &str, chat_id: &str, posts: &[Post], header: &str) -> Result<()> {
    let text = format_posts(posts, header);
    if text.is_empty() {
        return Ok(());
    }
    send_message(token, chat_id, &text)
}

/// 도움말
pub fn help_text() -> String {
    "<b>사용 가능한 명령어</b>\n\n\
     /status — 현재 상태 확인\n\
     /today — 오늘 실행 기록\n\
     /run — 지금 즉시 수집\n\
     /pause — 알림 일시정지\n\
     /resume — 알림 재개\n\
     /help — 이 도움말"
        .to_string()
}

/// 쌓인 명령을 가져온다. (명령 목록, 다음 offset)
pub fn fetch_commands(token: &str, offset: Option<i64>) -> Result<(Vec<(String, i64)>, i64)> {
    let mut params = vec![("limit", "50".to_string()), ("timeout", "0".to_string())];
    if let Some(off) = offset {
        params.push(("offset", off.to_string()));
    }
    let value = call(token, "getUpdates", &params)?;

    let mut commands: Vec<(String, i64)> = Vec::new();
    let mut next_offset = offset.unwrap_or(0);

    if let Some(results) = value.get("result").and_then(|v| v.as_array()) {
        for update in results {
            if let Some(id) = update.get("update_id").and_then(|v| v.as_i64()) {
                next_offset = id + 1;
            }
            let Some(message) = update.get("message") else {
                continue;
            };
            let Some(text) = message.get("text").and_then(|v| v.as_str()) else {
                continue;
            };
            if !text.starts_with('/') {
                continue;
            }
            let command = text
                .split_whitespace()
                .next()
                .unwrap_or("")
                .split('@')
                .next()
                .unwrap_or("")
                .to_lowercase();
            let Some(chat_id) = message
                .get("chat")
                .and_then(|c| c.get("id"))
                .and_then(|v| v.as_i64())
            else {
                continue;
            };
            commands.push((command, chat_id));
        }
    }
    Ok((commands, next_offset))
}

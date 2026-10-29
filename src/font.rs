//! 한글 폰트 로딩
//!
//! egui 기본 폰트에는 한글 글리프가 없어 네모(□)로 표시된다.
//! 시스템에 설치된 한글 폰트를 런타임에 읽어 등록한다.
//! (파일을 exe 에 임베드하지 않으므로 바이너리 크기가 늘지 않는다)

use eframe::egui;
use std::path::PathBuf;

/// 한글 폰트 후보 경로 (우선순위 순)
fn korean_font_candidates() -> Vec<PathBuf> {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
    let fonts = PathBuf::from(windir).join("Fonts");

    vec![
        // Windows 기본 한글 폰트
        fonts.join("malgun.ttf"),   // 맑은 고딕
        fonts.join("malgunbd.ttf"), // 맑은 고딕 Bold
        fonts.join("gulim.ttc"),    // 굴림
        fonts.join("batang.ttc"),   // 바탕
        fonts.join("NanumGothic.ttf"),
        // 사용자 폰트 폴더 (나눔고딕 등 직접 설치한 경우)
        dirs::font_dir().unwrap_or_default().join("NanumGothic.ttf"),
        dirs::font_dir().unwrap_or_default().join("malgun.ttf"),
    ]
}

/// 한글 폰트를 egui 컨텍스트에 등록한다.
/// 성공하면 사용한 폰트 경로를 반환한다.
pub fn install_korean_font(ctx: &egui::Context) -> Option<PathBuf> {
    for path in korean_font_candidates() {
        if !path.exists() {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };

        let mut fonts = egui::FontDefinitions::default();

        // 폰트 데이터 등록 (ttc 는 egui 가 지원하지 않으므로 ttf 만)
        if path.extension().and_then(|e| e.to_str()) != Some("ttf") {
            continue;
        }
        fonts
            .font_data
            .insert("korean".to_string(), egui::FontData::from_owned(bytes).into());

        // 비례 글꼴과 고정폭 글꼴 모두에 한글을 최우선으로 넣는다
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .insert(0, "korean".to_string());
        }

        ctx.set_fonts(fonts);
        return Some(path);
    }
    None
}

/// 폰트 로딩 결과 요약
pub fn font_status() -> String {
    match installed_font() {
        Some(path) => format!("한글 폰트: {}", path.display()),
        None => "한글 폰트를 찾지 못했습니다. (글자가 네모로 보일 수 있습니다)".to_string(),
    }
}

/// 설치된 한글 폰트 경로 (표시용)
pub fn installed_font() -> Option<PathBuf> {
    korean_font_candidates()
        .into_iter()
        .find(|p| p.exists() && p.extension().and_then(|e| e.to_str()) == Some("ttf"))
}

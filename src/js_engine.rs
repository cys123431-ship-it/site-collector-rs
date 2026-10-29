//! JavaScript 렌더링 엔진 (선택 사항)
//!
//! 경량 배포를 위해 기본 포함하지 않는다.
//! 사용자가 옵션으로 설치하면 JS 로 렌더링된 HTML 을 가져올 수 있다.
//!
//! 설치 방법 (둘 중 하나):
//!   1) Python + Playwright 설치 후 헬퍼 스크립트 사용
//!   2) Node.js + puppeteer 설치 후 헬퍼 스크립트 사용
//!
//! 이 모듈은 외부 실행 파일을 호출하는 방식이라 본체 크기가 늘지 않는다.

use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct JsEngine {
    pub available: bool,
    pub kind: String,
    pub detail: String,
}

impl JsEngine {
    /// 설치된 엔진을 탐지한다.
    pub fn detect() -> Self {
        // 1) 프로젝트 폴더의 헬퍼 스크립트 우선
        if let Some(engine) = Self::detect_python_playwright() {
            return engine;
        }
        if let Some(engine) = Self::detect_node_puppeteer() {
            return engine;
        }
        Self {
            available: false,
            kind: "none".to_string(),
            detail: "JavaScript 엔진이 설치되지 않았습니다.".to_string(),
        }
    }

    fn helper_dir() -> PathBuf {
        crate::config::app_dir().join("js_engine")
    }

    fn detect_python_playwright() -> Option<Self> {
        let script = Self::helper_dir().join("render.py");
        if !script.exists() {
            return None;
        }
        // python 명령 확인
        for exe in ["python", "py", "python3"] {
            if Command::new(exe).arg("--version").output().is_ok() {
                return Some(Self {
                    available: true,
                    kind: "python-playwright".to_string(),
                    detail: format!("Python + Playwright ({exe})"),
                });
            }
        }
        None
    }

    fn detect_node_puppeteer() -> Option<Self> {
        let script = Self::helper_dir().join("render.js");
        if !script.exists() {
            return None;
        }
        if Command::new("node").arg("--version").output().is_ok() {
            return Some(Self {
                available: true,
                kind: "node-puppeteer".to_string(),
                detail: "Node.js + Puppeteer".to_string(),
            });
        }
        None
    }

    /// 브라우저로 페이지를 열어 렌더링된 HTML 을 가져온다.
    pub fn fetch(&self, url: &str) -> Result<String> {
        match self.kind.as_str() {
            "python-playwright" => {
                let script = Self::helper_dir().join("render.py");
                let out = Command::new("python")
                    .arg(&script)
                    .arg(url)
                    .output()?;
                if !out.status.success() {
                    let err = String::from_utf8_lossy(&out.stderr);
                    return Err(anyhow!("Playwright 렌더링 실패: {}", err.trim()));
                }
                Ok(String::from_utf8_lossy(&out.stdout).to_string())
            }
            "node-puppeteer" => {
                let script = Self::helper_dir().join("render.js");
                let out = Command::new("node").arg(&script).arg(url).output()?;
                if !out.status.success() {
                    let err = String::from_utf8_lossy(&out.stderr);
                    return Err(anyhow!("Puppeteer 렌더링 실패: {}", err.trim()));
                }
                Ok(String::from_utf8_lossy(&out.stdout).to_string())
            }
            _ => Err(anyhow!("사용 가능한 JS 엔진이 없습니다.")),
        }
    }
}

//! 처리 이력 저장소
//!
//! data/state.json 에 기록한다.
//!   seen    : 이미 처리한 글 (uid → 날짜)
//!   history : 실행 기록
//!   paused  : 알림 일시정지 여부

use anyhow::Result;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

/// 실행 기록 1건
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub at: String,
    pub count: usize,
    pub sent: bool,
    #[serde(default)]
    pub report: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StateData {
    #[serde(default)]
    seen: BTreeMap<String, String>,
    #[serde(default)]
    history: Vec<RunRecord>,
    #[serde(default)]
    paused: bool,
}

impl Default for StateData {
    fn default() -> Self {
        Self {
            seen: BTreeMap::new(),
            history: Vec::new(),
            paused: false,
        }
    }
}

pub struct StateStore {
    path: PathBuf,
    data: StateData,
}

impl StateStore {
    pub fn load() -> Self {
        let path = crate::config::data_dir().join("state.json");
        let data = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<StateData>(&text).ok())
            .unwrap_or_default();
        Self { path, data }
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, serde_json::to_string_pretty(&self.data)?)?;
        Ok(())
    }

    /// 이미 처리한 글인지
    pub fn is_seen(&self, uid: &str) -> bool {
        self.data.seen.contains_key(uid)
    }

    /// 처리한 글로 기록
    pub fn mark_seen(&mut self, uids: &[String]) {
        let today = Local::now().format("%Y-%m-%d").to_string();
        for uid in uids {
            self.data.seen.insert(uid.clone(), today.clone());
        }
    }

    /// 실행 기록 추가
    pub fn record_run(&mut self, count: usize, sent: bool, report: &str) {
        self.data.history.push(RunRecord {
            at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            count,
            sent,
            report: report.to_string(),
        });
        // 최근 500건만 유지
        if self.data.history.len() > 500 {
            let drain = self.data.history.len() - 500;
            self.data.history.drain(0..drain);
        }
    }

    /// 마지막 실행 기록
    pub fn last_run(&self) -> Option<&RunRecord> {
        self.data.history.last()
    }

    /// 오늘 실행 기록
    pub fn today_runs(&self) -> Vec<&RunRecord> {
        let today = Local::now().format("%Y-%m-%d").to_string();
        self.data
            .history
            .iter()
            .filter(|r| r.at.starts_with(&today))
            .collect()
    }

    /// 전체 실행 기록
    pub fn history(&self) -> &[RunRecord] {
        &self.data.history
    }

    /// 처리한 글 수
    pub fn seen_count(&self) -> usize {
        self.data.seen.len()
    }

    pub fn paused(&self) -> bool {
        self.data.paused
    }

    pub fn set_paused(&mut self, value: bool) -> Result<()> {
        self.data.paused = value;
        self.save()
    }

    /// 오래된 기록 정리 (400일 초과)
    pub fn prune(&mut self) {
        let cutoff = (Local::now() - chrono::Duration::days(400))
            .format("%Y-%m-%d")
            .to_string();
        let cutoff_value = cutoff.clone();
        self.data
            .seen
            .retain(|_, date| date.as_str() >= cutoff_value.as_str());
        self.data.history.retain(|r| r.at >= cutoff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_roundtrip() {
        let mut data = StateData::default();
        data.seen.insert("a:1".to_string(), "2026-10-29".to_string());
        assert!(data.seen.contains_key("a:1"));
    }

    #[test]
    fn history_cap() {
        let mut data = StateData::default();
        for i in 0..600 {
            data.history.push(RunRecord {
                at: format!("2026-10-29 00:00:{:02}", i % 60),
                count: i,
                sent: false,
                report: String::new(),
            });
        }
        if data.history.len() > 500 {
            let drain = data.history.len() - 500;
            data.history.drain(0..drain);
        }
        assert_eq!(data.history.len(), 500);
    }
}

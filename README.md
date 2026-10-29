# 사이트 수집기 (Site Collector)

주소와 키워드만 넣으면 **자동으로 게시판 구조를 감지**해서, 키워드에 걸린 글을 모아 주는 데스크톱 앱입니다.

Rust 로 만들어 **단일 exe 6.7MB** 로 배포되며, 런타임 설치가 필요 없습니다.

---

## 다운로드

[Releases](https://github.com/cys123431-ship-it/site-collector-rs/releases) 에서 `site-collector.exe` 를 받아 실행하세요.

## 핵심 특징

| 항목 | 내용 |
|---|---|
| **자동 구조 감지** | 주소만 넣으면 게시글 링크·제목·날짜·본문·페이지 이동을 스스로 찾아냄 |
| **다중 사이트** | 원하는 만큼 주소를 등록 (각각 키워드 지정 가능) |
| **경량** | Rust + egui. **단일 exe 6.7MB**, Python 런타임 불필요 |
| **Markdown 리포트** | 워드·옵시디언·GitHub 에서 바로 열림 |
| **텔레그램 알림** | 제목+링크만 전송, 명령어로 제어 |
| **자동 실행** | Windows 작업 스케줄러 등록 |
| **중복 방지** | 처리한 글 기록으로 재발송 차단 |
| **JS 사이트 옵션** | Playwright/Puppeteer 를 설치하면 동적 사이트도 지원 |
| **한글 폰트** | 시스템 폰트(맑은 고딕) 런타임 로딩, 네모(□) 없음 |

## 빠른 시작

### 방법 1 — 미리 빌드된 exe

Releases 에서 `site-collector.exe` 다운로드 → 실행

### 방법 2 — 직접 빌드

```bat
cargo build --release
:: 결과물: target\release\site-collector.exe
```

또는

```bat
release.bat    :: 테스트 + 빌드 + dist 폴더 생성
run.bat        :: 빌드된 exe 실행
```

### 사용 흐름

1. GUI → **사이트 관리** 탭
2. 주소 입력 (예: `https://www.pcu.ac.kr/kor/article/Notify_1`)
3. **🔍 자동 분석** 클릭
4. 감지 결과 확인 (게시글 N건 · 날짜/본문 인식 여부)
5. 키워드 입력 (쉼표 구분, 비우면 전체)
6. **➕ 추가**
7. **수집 실행** 탭 → **▶ 수집 시작**

## CLI

```bat
site-collector.exe                       :: GUI 실행
site-collector.exe detect <URL>          :: 주소 구조 분석
site-collector.exe run --days 2          :: 수집 실행
site-collector.exe run --no-telegram     :: 알림 없이 수집
site-collector.exe status                :: 상태 확인
site-collector.exe commands              :: 텔레그램 명령 처리
site-collector.exe --version
```

## 자동 감지 원리

```
① HTML 수집 (정적 → 실패 시 브라우저)
② <a href> 와 onclick="fn('123')" 를 패턴별로 그룹화
③ 가장 많이 반복되는 패턴 = 게시글 링크
④ 부모 요소(li/tr/article)에서 날짜 탐색
   · .date 같은 날짜 전용 클래스 우선
   · 제목의 "2026-2학기" 를 날짜로 오인하지 않도록 연도 포함 패턴 우선
⑤ 내비게이션 영역(gnb/lnb/nav/menu) 제외
⑥ 상세 1건을 열어 본문 위치 추정
⑦ 페이지 이동 규칙 추출 (?page=2 등)
```

### 지원하는 두 가지 링크 방식

| 방식 | 예시 | 상세 URL |
|---|---|---|
| href 기반 | `<a href="/notice/123">제목</a>` | 그대로 사용 |
| onclick 기반 | `<a onclick="article.view('123')">제목</a>` | `{base}/{id}` 로 추론 |

onclick 방식은 상세 URL 을 알 수 없으므로, 후보 템플릿을 실제로 시도해 **열리는 것을 자동 선택**합니다.

## 검증 결과

| 사이트 | 결과 |
|---|---|
| 배재대 일반공지 | ✅ 14건 감지 · 날짜 인식 · 본문 인식 · 페이지 이동 인식 |
| 배재대 학과공지 | ✅ 10건 감지 · 날짜 인식 · 본문 인식 · 페이지 이동 인식 |
| Hacker News | ✅ 40건 감지 (제목 선택자 보정 필요) |

**테스트**: 10개 통과 (단위 6 + 통합 4)

## 텔레그램 설정

1. 텔레그램에서 `@BotFather` → `/newbot` → **토큰** 복사
2. 만든 봇 대화방에서 **`/start` 전송** (필수)
3. GUI → **텔레그램** 탭 → 토큰 붙여넣기 → **자동 찾기**
4. **테스트 메시지** 로 확인

### 명령어

| 명령 | 동작 |
|---|---|
| `/status` | 상태 확인 (마지막 실행, 등록 사이트, 일시정지 여부) |
| `/today` | 오늘 실행 기록 |
| `/run` | 지금 즉시 수집 |
| `/pause` | 알림 일시정지 |
| `/resume` | 알림 재개 |
| `/help` | 도움말 |

## 키워드 필터 모드

| 모드 | 조건 |
|---|---|
| **전부** | 수집된 글 모두 (기본) |
| **키워드 단독** | 추가 키워드에 걸린 글만 |
| **OR 조건** | 기본 키워드 **또는** 추가 키워드 |
| **AND 조건** | 기본 키워드 **이면서** 추가 키워드 |

키워드가 비어 있으면 자동으로 "전부"로 동작합니다.

## JavaScript 사이트 (선택)

React/Vue 같은 동적 사이트는 옵션 엔진이 필요합니다. **경량 배포를 위해 기본 미포함**이며, 설치하면 자동 인식됩니다.

```bat
:: 방법 1 — Python + Playwright
pip install playwright
playwright install chromium
copy js_engine\render.py.example js_engine\render.py

:: 방법 2 — Node.js + Puppeteer
npm install puppeteer
copy js_engine\render.js.example js_engine\render.js
```

자세한 내용은 [js_engine/README.md](js_engine/README.md) 를 참고하세요.

> Chromium 은 약 300MB 입니다. 필요할 때만 설치하세요.

## 파일 구성

| 파일 | 역할 |
|---|---|
| `src/main.rs` | CLI 진입점, GUI 실행 |
| `src/lib.rs` | 라이브러리 루트 |
| `src/gui.rs` | egui GUI (5개 탭) |
| `src/detect.rs` | **자동 구조 감지 엔진** (핵심) |
| `src/crawler.rs` | 감지된 구조로 실제 수집 |
| `src/fetch.rs` | HTTP (정적) |
| `src/js_engine.rs` | JS 엔진 옵션 연동 |
| `src/filter.rs` | 키워드 필터 (4모드) |
| `src/report.rs` | Markdown 리포트 |
| `src/telegram.rs` | 텔레그램 API |
| `src/state.rs` | 중복 방지, 실행 이력 |
| `src/config.rs` | 설정 (config.json, .env) |
| `src/font.rs` | 한글 폰트 로딩 |
| `src/pipeline.rs` | 전체 파이프라인 |
| `tests/detect_tests.rs` | 통합 테스트 |

생성되는 폴더 (git 제외):
```
config.json      설정
.env             봇 토큰 (커밋 금지)
output/          Markdown 리포트
data/state.json  중복 방지 이력
dist/            배포용 (release.bat 결과)
```

## 개발

```bat
cargo check            :: 빠른 타입 검사
cargo test             :: 테스트 (10개)
cargo build            :: 디버그 빌드
cargo build --release  :: 최적화 빌드 (약 2분)
release.bat            :: 테스트 + 빌드 + dist 생성
```

### 크기 최적화 설정 (Cargo.toml)

```toml
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

| 빌드 | 크기 |
|---|---|
| debug | 205MB |
| **release** | **6.69MB** |

## 주의사항

- **서버 예절**: 요청 사이 기본 0.4초 지연. `config.json` 의 `crawl.delay_ms` 로 조정
- **자동 감지 정확도**: 70~90%. 실패하면 "감지 실패"로 표시되며, 사이트 구조가 바뀌면 재분석 필요
- **이용약관**: 크롤링을 허용하지 않는 사이트도 있습니다. robots.txt 와 약관을 확인하세요
- **로그인 필요 페이지**: 지원하지 않습니다

## 라이선스

MIT License — [LICENSE](LICENSE)

## 관련 프로젝트

배재대학교 전용 프로그램은 [pcu-cse-notice-collector](https://github.com/cys123431-ship-it/pcu-cse-notice-collector) 에서 관리합니다.

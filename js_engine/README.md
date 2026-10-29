# JavaScript 엔진 (선택 사항)

React/Vue/Next.js 같은 **동적 사이트**를 수집하려면 브라우저 엔진이 필요합니다.

본체는 이 폴더가 비어 있어도 정상 동작합니다(정적 HTML 만 수집).
필요할 때만 아래 방법 중 하나로 설치하세요.

---

## 방법 1 — Python + Playwright

```bat
pip install playwright
playwright install chromium
```

그다음 `render.py.example` 을 **`render.py`** 로 복사합니다.

```bat
copy render.py.example render.py
```

## 방법 2 — Node.js + Puppeteer

```bat
npm install puppeteer
```

그다음 `render.js.example` 을 **`render.js`** 로 복사합니다.

```bat
copy render.js.example render.js
```

---

## 확인 방법

설치 후 GUI 의 **정보** 탭에서 JavaScript 엔진 상태를 확인할 수 있습니다.

```
JavaScript 엔진 (선택)
  Python + Playwright (python)      ← 초록색이면 정상
```

## 사용 방법

1. 설치 후 GUI 재시작
2. **사이트 관리** 탭에서 주소 입력 → **자동 분석**
3. "JavaScript 렌더링이 필요한 페이지로 보입니다" 로 표시되면
4. `config.json` 의 `crawl.use_js_engine` 을 `true` 로 바꾸거나
   CLI 에서 `--js` 옵션 사용

## 주의

- Chromium 은 약 **300MB** 입니다. 디스크 공간을 확인하세요.
- 렌더링은 페이지당 **2~3초** 걸립니다(정적 수집은 0.3초).
- 로그인이 필요한 페이지는 지원하지 않습니다.
- `render.py` / `render.js` 는 `.gitignore` 에 등록되어 있어 커밋되지 않습니다.

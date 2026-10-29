//! 통합 테스트: 자동 감지 엔진 (오프라인, 파싱 로직만)

use site_collector_rs_lib::detect_helpers::*;

#[test]
fn patternize_replaces_numbers() {
    // 2자리 이상 숫자는 {id}
    assert_eq!(patternize("/notice/123"), "/notice/{id}");
    // 1자리 숫자는 보존
    assert_eq!(patternize("/page/2"), "/page/0");
    // 숫자 없으면 그대로
    assert_eq!(patternize("/about"), "/about");
}

#[test]
fn pattern_matches_works() {
    assert!(pattern_matches("/notice/{id}", "/notice/123"));
    assert!(pattern_matches("/notice/{id}", "/notice/1"));
    assert!(!pattern_matches("/notice/{id}", "/notice/abc"));
    assert!(!pattern_matches("/notice/{id}", "/other/123"));
    assert!(pattern_matches("item?id={id}", "item?id=50282838"));
}

#[test]
fn build_detail_url_templates() {
    let base = "https://example.com/notice";

    assert_eq!(
        build_detail_url("{base}/{id}", base, "123"),
        "https://example.com/notice/123"
    );
    assert_eq!(
        build_detail_url("{base}?articleNo={id}", base, "456"),
        "https://example.com/notice?articleNo=456"
    );
    // 쿼리스트링이 있는 페이지에서도 base 는 깨끗하게
    assert_eq!(
        build_detail_url("{base}/{id}", "https://example.com/notice?page=2", "789"),
        "https://example.com/notice/789"
    );
}

#[test]
fn build_page_url_patterns() {
    let base = "https://example.com/notice";

    // 1페이지는 원본 그대로
    assert_eq!(build_page_url(base, "?page={n}", 1), base);

    // 쿼리 패턴
    assert_eq!(
        build_page_url(base, "?page={n}", 2),
        "https://example.com/notice?page=2"
    );

    // 경로 패턴
    assert_eq!(
        build_page_url(base, "/page/{n}", 3),
        "https://example.com/notice/page/3"
    );

    // 패턴 없으면 원본
    assert_eq!(build_page_url(base, "", 5), base);
}

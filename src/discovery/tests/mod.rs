mod advertiser;
mod parser_tests;
mod raop;

#[tokio::test]
async fn test_scan_with_timeout() {
    use std::time::Duration;

    use super::scan;

    // This test attempts to scan. It should not fail, but may return empty list.
    let result = scan(Duration::from_millis(100)).await;
    assert!(result.is_ok());
}
mod advertiser_extra;

use super::Headers;
use super::headers::names;

#[test]
fn test_new_headers_is_empty() {
    let headers = Headers::new();
    assert!(headers.is_empty());
    assert_eq!(headers.len(), 0);
}

#[test]
fn test_insert_and_get_case_insensitive() {
    let mut headers = Headers::new();
    headers.insert("Content-Type", "application/json");

    assert_eq!(headers.get("Content-Type"), Some("application/json"));
    assert_eq!(headers.get("content-type"), Some("application/json"));
    assert_eq!(headers.get("CONTENT-TYPE"), Some("application/json"));
}

#[test]
fn test_contains_case_insensitive() {
    let mut headers = Headers::new();
    headers.insert("X-Test", "True");

    assert!(headers.contains("X-Test"));
    assert!(headers.contains("x-test"));
    assert!(!headers.contains("X-Other"));
}

#[test]
fn test_overwrite_behavior() {
    let mut headers = Headers::new();
    headers.insert("Test-Header", "Value1");
    // This should overwrite "Test-Header" because they are case-insensitively equal
    headers.insert("test-header", "Value2");

    let val = headers.get("Test-Header").unwrap();
    assert_eq!(val, "Value2");

    // Ensure only one entry exists
    assert_eq!(headers.len(), 1);
}

#[test]
fn test_cseq_parsing() {
    let mut headers = Headers::new();
    assert_eq!(headers.cseq(), None);

    headers.insert("CSeq", "10");
    assert_eq!(headers.cseq(), Some(10));

    headers.insert("CSeq", "invalid");
    assert_eq!(headers.cseq(), None);
}

#[test]
fn test_content_length_parsing() {
    let mut headers = Headers::new();
    assert_eq!(headers.content_length(), None);

    headers.insert("Content-Length", "123");
    assert_eq!(headers.content_length(), Some(123));

    headers.insert("Content-Length", "abc");
    assert_eq!(headers.content_length(), None);
}

#[test]
fn test_session_parsing() {
    let mut headers = Headers::new();
    headers.insert("Session", "SESSION_ID");
    assert_eq!(headers.session(), Some("SESSION_ID"));
}

#[test]
fn test_iter() {
    let mut headers = Headers::new();
    headers.insert("A", "1");
    headers.insert("B", "2");

    let vec: Vec<(&str, &str)> = headers.iter().collect();
    assert_eq!(vec.len(), 2);
    // Sort to ensure order for assertion
    let mut vec_sorted = vec;
    vec_sorted.sort_by_key(|k| k.0);

    assert_eq!(vec_sorted[0], ("A", "1"));
    assert_eq!(vec_sorted[1], ("B", "2"));
}

#[test]
fn test_from_iterator() {
    let data = vec![
        ("H1".to_string(), "V1".to_string()),
        ("H2".to_string(), "V2".to_string()),
        ("h1".to_string(), "V3".to_string()), // Should overwrite H1
    ];
    let headers: Headers = data.into_iter().collect();

    assert_eq!(headers.len(), 2);
    assert_eq!(headers.get("H1"), Some("V3"));
    assert_eq!(headers.get("H2"), Some("V2"));
}

#[test]
fn test_constants() {
    assert_eq!(names::CSEQ, "CSeq");
    assert_eq!(names::CONTENT_TYPE, "Content-Type");
}

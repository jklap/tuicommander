use tuic_ipc::http::ResponseDecoder;

// Catches: an interim 100 Continue is returned as the final response and the real
// reply that follows it is dropped (RFC 9112 6.3: 1xx other than 101 is interim).
#[test]
fn interim_100_continue_is_skipped_for_the_final_response() {
    let mut decoder = ResponseDecoder::default();
    decoder.push(b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
    let response = decoder
        .response(false)
        .unwrap()
        .expect("final response is complete");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, "ok");
}

// Catches: the HEAD flag leaks into other methods, so a GET body is silently emptied.
#[test]
fn non_head_request_keeps_its_content_length_body() {
    for method in ["GET", "POST", "DELETE"] {
        let mut decoder = ResponseDecoder::for_request(method);
        decoder.push(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhel");
        assert!(
            decoder.response(false).unwrap().is_none(),
            "{method}: body still pending"
        );
        decoder.push(b"lo");
        assert_eq!(decoder.response(false).unwrap().unwrap().body, "hello");
    }
}

// Catches: a 204/304 short-circuit that also swallows a truncated header section at EOF.
#[test]
fn bodyless_status_with_truncated_headers_at_eof_is_an_error() {
    for status in [204, 304] {
        let mut decoder = ResponseDecoder::default();
        decoder.push(format!("HTTP/1.1 {status} X\r\nConnection: keep-alive\r\n").as_bytes());
        assert!(decoder.response(false).unwrap().is_none());
        assert!(decoder.response(true).is_err());
    }
}

// Catches: status-class check applied to a non-1xx neighbour (199/200, 203/205, 303/305).
#[test]
fn statuses_next_to_the_bodyless_set_still_wait_for_their_body() {
    for status in [200, 203, 205, 303, 305, 99] {
        let mut decoder = ResponseDecoder::default();
        decoder.push(format!("HTTP/1.1 {status} X\r\nContent-Length: 3\r\n\r\n").as_bytes());
        assert!(
            decoder.response(false).unwrap().is_none(),
            "{status} must wait"
        );
        assert!(decoder.response(true).is_err(), "{status} truncated");
    }
}

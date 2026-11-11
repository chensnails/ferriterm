use super::*;

fn endpoint() -> Endpoint {
    Endpoint {
        version: "1.0.0".to_string(),
        port: 1,
        token: "abc".to_string(),
        pid: 1,
    }
}

#[test]
fn an_auth_failure_gains_a_restart_hint() {
    let hinted = Client::auth_hint(ClientError {
        status: 401,
        kind: "auth".to_string(),
        message: "the endpoint token is missing or wrong".to_string(),
    });
    assert!(hinted.message.contains("ferriterm-cli status"));
}

#[test]
fn other_failures_are_left_alone() {
    let plain = Client::auth_hint(ClientError {
        status: 409,
        kind: "backend".to_string(),
        message: "no session gone".to_string(),
    });
    assert_eq!(plain.message, "no session gone");
}

#[test]
fn a_response_without_headers_is_a_transport_error() {
    let client = Client::new(&endpoint());
    let error = client
        .interpret::<Health>(b"nonsense without a header block")
        .unwrap_err();
    assert_eq!(error.status, 0);
    assert!(error.message.contains("header block"), "{}", error.message);
}

#[test]
fn an_error_answer_keeps_the_status_and_the_message() {
    let client = Client::new(&endpoint());
    let body = br#"{"error":"no session gone","kind":"backend"}"#;
    let mut response = b"HTTP/1.1 409 Conflict\r\nContent-Length: 43\r\n\r\n".to_vec();
    response.extend_from_slice(body);
    let error = client.interpret::<Health>(&response).unwrap_err();
    assert_eq!(error.status, 409);
    assert_eq!(error.kind, "backend");
    assert_eq!(error.message, "no session gone");
}

#[test]
fn a_good_answer_is_decoded() {
    let client = Client::new(&endpoint());
    let body = br#"{"version":"1.0.0","pid":7,"sessions":2,"uptimeSecs":3,"ready":true}"#;
    let mut response = b"HTTP/1.1 200 OK\r\nContent-Length: 60\r\n\r\n".to_vec();
    response.extend_from_slice(body);
    let health = client.interpret::<Health>(&response).unwrap();
    assert_eq!(health.sessions, 2);
    assert!(health.ready);
}

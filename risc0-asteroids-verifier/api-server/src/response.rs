use actix_web::{http::StatusCode, HttpResponse};

pub(crate) fn json_error_with_code(
    status: StatusCode,
    message: impl Into<String>,
    error_code: Option<&str>,
) -> HttpResponse {
    let mut body = serde_json::json!({
        "success": false,
        "error": message.into(),
    });
    if let Some(code) = error_code {
        body["error_code"] = serde_json::Value::String(code.to_string());
    }
    HttpResponse::build(status).json(body)
}

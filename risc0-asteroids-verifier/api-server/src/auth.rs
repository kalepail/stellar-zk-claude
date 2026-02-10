use actix_web::http::header::{HeaderMap, AUTHORIZATION};

pub(crate) fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let authorization = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = authorization.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }

    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed)
}

pub(crate) fn is_request_authorized(headers: &HeaderMap, expected_api_key: Option<&str>) -> bool {
    let Some(expected_api_key) = expected_api_key else {
        return true;
    };

    let x_api_key = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim);
    if x_api_key == Some(expected_api_key) {
        return true;
    }

    bearer_token(headers).is_some_and(|token| token == expected_api_key)
}

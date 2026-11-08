//! OpenSubtitles REST client.

use std::io::Read;
use std::time::Duration;

use serde_json::Value;

/// Base endpoint for the OpenSubtitles v1 REST API.
pub const BASE: &str = "https://api.opensubtitles.com/api/v1/";

/// User-Agent required by the OpenSubtitles API and CDN.
pub const USER_AGENT: &str = "LegendasLinux v1.0";

/// Outgoing HTTP request representation for OpenSubtitles API calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

impl ApiRequest {
    /// Looks up a header value case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Raw error returned by an HTTP transport before mapping to user-facing messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    Status(u16),
    Network(String),
}

/// Builds an [`ApiRequest`] for the given OpenSubtitles route, API key, JSON payload, and bearer token.
pub fn build_request(
    route: &str,
    key: &str,
    payload: Option<&Value>,
    token: &str,
) -> ApiRequest {
    let mut headers = vec![
        ("Api-Key".to_string(), key.to_string()),
        ("User-Agent".to_string(), USER_AGENT.to_string()),
        ("Accept".to_string(), "application/json".to_string()),
    ];
    if !token.is_empty() {
        headers.push(("Authorization".to_string(), format!("Bearer {token}")));
    }

    let (method, body) = if let Some(json_payload) = payload {
        headers.push(("Content-Type".to_string(), "application/json".to_string()));
        (
            "POST".to_string(),
            Some(serde_json::to_vec(json_payload).expect("valid JSON value")),
        )
    } else {
        ("GET".to_string(), None)
    };

    ApiRequest {
        method,
        url: format!("{BASE}{route}"),
        headers,
        body,
    }
}

/// Maps a transport error from OpenSubtitles to a localized Portuguese message.
pub fn map_api_error(err: TransportError) -> String {
    match err {
        TransportError::Status(401) => "Credenciais inválidas.".to_string(),
        TransportError::Status(403) => "Acesso negado: confira a chave e a conta.".to_string(),
        TransportError::Status(406) => "Limite de downloads atingido.".to_string(),
        TransportError::Status(429) => "Muitas requisições. Tente mais tarde.".to_string(),
        TransportError::Status(code) => format!("OpenSubtitles retornou HTTP {code}."),
        TransportError::Network(_) => {
            "Não foi possível conectar ao OpenSubtitles. Confira a internet e tente novamente."
                .to_string()
        }
    }
}

/// Executes an OpenSubtitles API request using the provided transport function.
pub fn api_with<F>(
    route: &str,
    key: &str,
    payload: Option<&Value>,
    token: &str,
    transport: F,
) -> Result<Value, String>
where
    F: FnOnce(&ApiRequest) -> Result<Vec<u8>, TransportError>,
{
    let req = build_request(route, key, payload, token);
    let bytes = transport(&req).map_err(map_api_error)?;
    serde_json::from_slice(&bytes)
        .map_err(|_| "Resposta inválida do OpenSubtitles.".to_string())
}

/// Performs a live HTTPS request to the OpenSubtitles REST API.
pub fn api(
    route: &str,
    key: &str,
    payload: Option<&Value>,
    token: &str,
) -> Result<Value, String> {
    api_with(route, key, payload, token, execute_request)
}

fn execute_request(req: &ApiRequest) -> Result<Vec<u8>, TransportError> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .build();

    let mut request = agent.request(&req.method, &req.url);
    for (k, v) in &req.headers {
        request = request.set(k, v);
    }

    let response = if let Some(body) = &req.body {
        request.send_bytes(body)
    } else {
        request.call()
    };

    match response {
        Ok(resp) => {
            let mut buf = Vec::new();
            resp.into_reader()
                .take(5 * 1024 * 1024)
                .read_to_end(&mut buf)
                .map_err(|e| TransportError::Network(e.to_string()))?;
            Ok(buf)
        }
        Err(ureq::Error::Status(code, _)) => Err(TransportError::Status(code)),
        Err(ureq::Error::Transport(err)) => Err(TransportError::Network(err.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_api_headers_and_payload() {
        let mut captured = None;
        let payload = json!({"username": "user"});
        let res = api_with("login", "key", Some(&payload), "token", |req| {
            captured = Some(req.clone());
            Ok(b"{\"token\":\"ok\"}".to_vec())
        })
        .unwrap();

        assert_eq!(res["token"], "ok");
        let req = captured.unwrap();
        assert_eq!(req.header("Authorization"), Some("Bearer token"));
        assert_eq!(req.header("Api-Key"), Some("key"));
        assert_eq!(req.header("User-Agent"), Some(USER_AGENT));
        assert_eq!(req.method, "POST");
    }

    #[test]
    fn test_network_error() {
        let err = api_with("subtitles", "key", None, "", |_| {
            Err(TransportError::Network("offline".to_string()))
        })
        .unwrap_err();
        assert!(err.contains("Confira a internet"));
    }

    #[test]
    fn test_quota_error() {
        let err = api_with("subtitles", "key", None, "", |_| {
            Err(TransportError::Status(429))
        })
        .unwrap_err();
        assert!(err.contains("Muitas requisições"));
    }
}

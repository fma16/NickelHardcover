use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use itertools::Itertools;
use jiff::Timestamp;
use retry::{
  OperationResult,
  delay::{Exponential, jitter},
  retry,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use ureq::{
  Body,
  http::{Response, StatusCode},
};

use crate::rate_limit::RateLimit;
use crate::{appcontext::AppContext, config::BASE_URL};
use crate::{commands::oauthset::refresh_token, utils::ExpectedError};

pub mod scalars {
  #![allow(non_camel_case_types)]

  use jiff::Timestamp;

  pub type date = String;
  pub type citext = String;
  pub type jsonb = serde_json::Value;
  pub type json = serde_json::Value;
  pub type numeric = f64;
  pub type float8 = f64;
  pub type bigint = i64;
  pub type smallint = i16;
  pub type timestamp = String;
  pub type timestamptz = Timestamp;
}

pub fn handle_response(context: &mut AppContext, res: Response<Body>) -> Result<Value> {
  let code = res.status();

  // Proxies can return plain text (or sensitive/truncated bodies). Classify
  // transient HTTP failures before parsing or logging their response bodies.
  if matches!(code.as_u16(), 408 | 500 | 502 | 503 | 504) {
    return Err(
      ExpectedError::Network(format!(
        "Hardcover is temporarily unavailable (HTTP {}). Please try syncing again later.",
        code.as_u16()
      ))
      .into(),
    );
  }
  if code == StatusCode::UNAUTHORIZED {
    return Err(ExpectedError::Unauthorized.into());
  }
  if code == StatusCode::FORBIDDEN {
    return Err(ExpectedError::BookInfo(
      "Hardcover denied access (HTTP 403). Reconnect with a token that grants the required permissions. Journal access requires read:journal; reviews require write:reviews.".into(),
    )
    .into());
  }

  if code == StatusCode::TOO_MANY_REQUESTS
    && let Some(retry_after) = res.headers().get("Retry-After")
  {
    let retry_after = retry_after
      .to_str()
      .context("Failed to get <i>Retry-After</i> header value")?
      .parse::<u64>()
      .context("Failed to parse <i>Retry-After</i> header")?;
    let duration = Duration::from_secs(retry_after);
    log::info!("Encountered Retry-After header sleeping for {}", duration.as_secs());
    sleep(duration);
    bail!("Rate limited, retrying");
  }

  let (daily, rate_limit): (Vec<_>, Vec<_>) = RateLimit::from_headers(res.headers())
    .context("Failed to parse rate limit")?
    .into_iter()
    .partition(|limit| limit.name().eq_ignore_ascii_case("daily"));

  if let Some(daily) = daily.first()
    && daily.remaining() == 0
  {
    return Err(ExpectedError::DailyRateLimit.into());
  }

  if let Some(rate_limit) = rate_limit.first() {
    context.rate_limit = rate_limit.remaining().max(1) as usize;

    if rate_limit.remaining() == 0 {
      let duration = rate_limit.reset().unwrap_or(Duration::from_secs(1));
      log::info!("Reached rate limit sleeping for {}", duration.as_secs());
      sleep(duration);
    }
  }

  let body = res.into_body().read_to_string().map_err(transport_error)?;

  // OAuth responses carry tokens and device codes, and every log level is
  // buffered and may be written to disk even with debug disabled. Only log
  // and report the body after redacting them; a body that is not JSON could
  // be a truncated token response, so report its size only.
  let json = serde_json::from_str::<Value>(&body)
    .with_context(|| format!("Failed to parse JSON response ({} bytes)", body.len()))?;
  let redacted = redact(json.clone()).to_string();

  log::debug!("{redacted}");

  let mut errors = Vec::<String>::new();
  collect_errors(&json, &mut errors);

  if errors.is_empty() && !code.is_success() {
    errors.push(redacted);
  }

  if !errors.is_empty() {
    let msg = format!("Request failed <i>{code}</i><br>{}", errors.join("<br>>"));
    log::error!("{msg}");

    bail!(msg);
  }

  Ok(json)
}

pub fn transport_error(error: ureq::Error) -> anyhow::Error {
  match error {
    ureq::Error::Timeout(_) => {
      ExpectedError::Network("Hardcover did not respond in time. Please try syncing again later.".into()).into()
    }
    ureq::Error::Io(_) | ureq::Error::HostNotFound | ureq::Error::ConnectionFailed => ExpectedError::Network(
      "Could not communicate with Hardcover. Please check your connection and try syncing again later.".into(),
    )
    .into(),
    other => other.into(),
  }
}

pub fn retry_request<T>(mut request: impl FnMut() -> Result<T>) -> Result<T> {
  retry(Exponential::from_millis(10).map(jitter).take(3), || match request() {
    Ok(value) => OperationResult::Ok(value),
    // Do not multiply the timeout or immediately replay a write whose outcome
    // is unknown. Expected errors go back to the caller (and the sync queue).
    Err(error) if error.downcast_ref::<ExpectedError>().is_some() => OperationResult::Err(error),
    Err(error) => OperationResult::Retry(error),
  })
  .map_err(|err| err.error)
}

fn redact(mut value: Value) -> Value {
  match &mut value {
    Value::Array(values) => values.iter_mut().for_each(|v| *v = redact(v.take())),
    Value::Object(map) => {
      for (key, v) in map.iter_mut() {
        *v = if ["access_token", "refresh_token", "id_token", "device_code", "user_code"].contains(&key.as_str()) {
          Value::from("[redacted]")
        } else {
          redact(v.take())
        };
      }
    }
    _ => {}
  }
  value
}

fn collect_errors(value: &serde_json::Value, errors: &mut Vec<String>) {
  match value {
    Value::Array(arr) => {
      for val in arr {
        collect_errors(val, errors);
      }
    }
    Value::Object(map) => {
      if let Some(val) = map.get("error").or(map.get("errors")) {
        if let Some(e) = val.as_str() {
          errors.push(
            map
              .get("error_description")
              .or(map.get("message"))
              .and_then(Value::as_str)
              .map_or_else(|| e.to_string(), |desc| format!("{e} — {desc}")),
          );
        } else if let Some(vec) = val.as_array() {
          errors.extend(vec.iter().filter_map(Value::as_str).map(ToString::to_string));
        }
      }

      for val in map.values() {
        collect_errors(val, errors);
      }
    }
    _ => {}
  }
}

pub fn send_request<T: Serialize, R: DeserializeOwned>(
  context: &mut AppContext,
  operation_name: &str,
  request_body: T,
) -> Result<R> {
  if context.config.access_token.is_empty() {
    return Err(ExpectedError::Unauthorized.into());
  }

  if let Some(token_expires_at) = context.config.token_expires_at
    && Timestamp::now().duration_until(token_expires_at).is_negative()
    && !context.config.refresh_token.is_empty()
  {
    refresh_token(context)?;
  }

  let json = retry_request(|| {
    let res = context
      .agent
      .post(format!("{}{}", BASE_URL, "/v1/graphql"))
      .header("authorization", format!("Bearer {}", context.config.access_token))
      .send_json(&request_body)
      .map_err(transport_error)
      .context("Failed to send request")?;

    handle_response(context, res)
  })
  .context(format!("<i>{operation_name}</i> request failed"))?;

  serde_json::from_value(json).context(format!("Failed to deserialize <i>{operation_name}</i> response"))
}

pub fn batch_requests<T: Serialize, R: DeserializeOwned>(
  context: &mut AppContext,
  operation_name: &str,
  request_bodies: &[T],
) -> Result<Vec<R>> {
  request_bodies
    .iter()
    .batching(|it| {
      let chunk = it.take(context.rate_limit).collect::<Vec<_>>();
      if chunk.is_empty() {
        None
      } else {
        log::debug!("Batching {}", chunk.len());
        Some(send_request::<_, Vec<R>>(context, operation_name, chunk))
      }
    })
    .flatten_ok()
    .collect()
}

#[cfg(test)]
mod tests {
  use argh::FromArgs;

  use super::*;
  use crate::{commands::oauthset::OAuthSet, config::Config, utils::LOGGER};

  #[test]
  fn server_unavailability_is_a_network_error_without_reading_the_body() {
    let mut context = AppContext {
      agent: ureq::Agent::new_with_defaults(),
      config: Config::default(),
      rate_limit: 1,
      user: None,
    };
    for status in [503, 408, 500, 502, 504] {
      let response = Response::builder()
        .status(status)
        .body(Body::builder().data("no available server SECRET"))
        .unwrap();
      let error = handle_response(&mut context, response).unwrap_err();
      assert_eq!(
        error.downcast_ref::<ExpectedError>().map(AsRef::as_ref),
        Some("Network"),
        "{error:#}"
      );
      assert!(!format!("{error:#}").contains("SECRET"));
    }
    for (status, expected) in [
      (200, None),
      (401, Some("Unauthorized")),
      (403, Some("BookInfo")),
      (400, None),
      (501, None),
    ] {
      let response = Response::builder()
        .status(status)
        .body(Body::builder().data("{}"))
        .unwrap();
      let result = handle_response(&mut context, response);
      assert_eq!(result.is_ok(), status == 200);
      assert_eq!(
        result
          .err()
          .as_ref()
          .and_then(|e| e.downcast_ref::<ExpectedError>())
          .map(AsRef::as_ref),
        expected
      );
    }
  }

  #[test]
  fn request_timeouts_are_not_replayed_or_wrapped_as_oauth_errors() {
    use crate::commands::{oauthrequest, oauthset};
    use std::{
      net::TcpListener,
      sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
      },
      thread,
    };

    // A real HTTP CONNECT proxy that accepts connections but never responds.
    // The production GraphQL and OAuth paths must all return Network once.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy = ureq::Proxy::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let shutdown = stop.clone();
    let server = thread::spawn(move || {
      let mut connections = Vec::new();
      while !shutdown.load(Ordering::SeqCst) {
        match listener.accept() {
          Ok((stream, _)) => connections.push(stream),
          Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => sleep(Duration::from_millis(1)),
          Err(error) => panic!("{error}"),
        }
      }
      connections.len()
    });
    let mut context = AppContext {
      agent: ureq::Agent::config_builder()
        .proxy(Some(proxy))
        .timeout_global(Some(Duration::from_millis(100)))
        .http_status_as_error(false)
        .build()
        .into(),
      config: Config {
        access_token: "test-token".into(),
        refresh_token: "test-refresh".into(),
        ..Config::default()
      },
      rate_limit: 1,
      user: None,
    };
    let results = [
      send_request::<_, Value>(&mut context, "TestWrite", serde_json::json!({"query": "mutation {}"})).map(|_| ()),
      oauthset::refresh_token(&mut context),
      oauthrequest::run(&mut context, &oauthrequest::OAuthRequest {}),
      oauthset::run(
        &mut context,
        &OAuthSet::from_args(&["oauth-set"], &["--device-code", "test-device"]).unwrap(),
      ),
    ];
    stop.store(true, Ordering::SeqCst);
    let connections = server.join().unwrap();
    for result in results {
      let error = result.unwrap_err();
      assert!(
        matches!(error.root_cause().downcast_ref(), Some(ExpectedError::Network(_))),
        "{error:#}"
      );
    }
    assert_eq!(connections, 4, "A timed-out request must not be replayed");
    assert_eq!(context.config.access_token, "test-token");
    assert_eq!(context.config.refresh_token, "test-refresh");
  }

  #[test]
  fn body_timeout_is_a_network_error_and_does_not_replay_the_request() {
    use std::{
      io::{BufRead, BufReader, Write},
      net::TcpListener,
      sync::mpsc,
      thread,
    };

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (stop, shutdown) = mpsc::channel();
    let server = thread::spawn(move || {
      let (mut stream, _) = listener.accept().unwrap();
      stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
      let mut reader = BufReader::new(stream.try_clone().unwrap());
      loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" {
          break;
        }
      }
      stream
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n")
        .unwrap();
      let _ = shutdown.recv_timeout(Duration::from_secs(5));
    });
    let mut context = AppContext {
      agent: ureq::Agent::config_builder()
        .proxy(None)
        .timeout_global(Some(Duration::from_millis(100)))
        .build()
        .into(),
      config: Config::default(),
      rate_limit: 1,
      user: None,
    };
    let mut attempts = 0;
    let result = retry_request(|| {
      attempts += 1;
      let response = context.agent.get(&url).call().map_err(transport_error)?;
      handle_response(&mut context, response)
    });
    stop.send(()).unwrap();
    server.join().unwrap();
    let error = result.unwrap_err();
    assert!(
      matches!(error.downcast_ref(), Some(ExpectedError::Network(_))),
      "{error:#}"
    );
    assert_eq!(attempts, 1);
    for error in [
      ureq::Error::HostNotFound,
      ureq::Error::ConnectionFailed,
      ureq::Error::Io(std::io::Error::other("sensitive detail")),
    ] {
      let error = transport_error(error);
      assert!(matches!(error.downcast_ref(), Some(ExpectedError::Network(_))));
      assert!(!error.to_string().contains("sensitive detail"));
    }
    assert!(
      transport_error(ureq::Error::BadUri("invalid".into()))
        .downcast_ref::<ExpectedError>()
        .is_none()
    );
  }

  #[test]
  fn missing_permissions_are_actionable_and_redacted() {
    let mut context = AppContext {
      agent: ureq::Agent::new_with_defaults(),
      config: Config::default(),
      rate_limit: 1,
      user: None,
    };
    for scope in ["read:journal", "write:reviews"] {
      let response = Response::builder()
        .status(403)
        .body(Body::builder().data(format!(
          r#"{{"error":"insufficient_scope","error_description":"Missing scopes: {scope}","access_token":"SECRET"}}"#
        )))
        .unwrap();
      let error = handle_response(&mut context, response).unwrap_err();
      let expected = error
        .downcast_ref::<ExpectedError>()
        .expect("Expected an actionable error");
      assert_eq!(expected.as_ref(), "BookInfo");
      assert!(expected.to_string().contains(scope), "{expected}");
      assert!(expected.to_string().contains("Reconnect"), "{expected}");
      assert!(!format!("{error:#}").contains("SECRET"));
    }
  }

  #[test]
  fn oauth_secrets_never_reach_errors_or_buffered_logs() {
    // As in `main`: every level is buffered and may be written to disk.
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Debug);
    let mut context = AppContext {
      agent: ureq::Agent::new_with_defaults(),
      config: Config::default(),
      rate_limit: 1,
      user: None,
    };
    let tokens = r#"{"access_token":"SECRET-A","refresh_token":"SECRET-R","expires_in":3600,"nested":[{"device_code":"SECRET-D","user_code":"SECRET-U"}]}"#;
    let respond = |status: u16, body: &str| {
      Response::builder()
        .status(status)
        .body(Body::builder().data(body.to_owned()))
        .unwrap()
    };

    // Callers still receive the real tokens they need to store.
    let json = handle_response(&mut context, respond(200, tokens)).unwrap();
    assert_eq!(json["refresh_token"], "SECRET-R");

    let error = format!("{:#}", handle_response(&mut context, respond(400, tokens)).unwrap_err());
    assert!(
      error.contains(r#""access_token":"[redacted]""#) && error.contains("3600"),
      "{error}"
    );
    let truncated = format!(
      "{:#}",
      handle_response(&mut context, respond(200, r#"{"access_token":"SECRET-A"#)).unwrap_err()
    );
    let log = LOGGER.buffer.lock().unwrap().clone();
    assert!(log.contains(r#""refresh_token":"[redacted]""#), "{log}");
    let args = OAuthSet::from_args(&["oauth-set"], &["--device-code", "SECRET-D"]).unwrap();
    for text in [error, truncated, format!("{args:?}"), log] {
      assert!(!text.contains("SECRET"), "{text}");
    }
  }
}

use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use itertools::Itertools;
use jiff::Timestamp;
use retry::{
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

  let body = res.into_body().read_to_string()?;

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

  let json = retry(Exponential::from_millis(10).map(jitter).take(3), || {
    let res = context
      .agent
      .post(format!("{}{}", BASE_URL, "/v1/graphql"))
      .header("authorization", format!("Bearer {}", context.config.access_token))
      .send_json(&request_body)
      .context("Failed to send request")?;

    if res.status() == StatusCode::UNAUTHORIZED {
      return Err(ExpectedError::Unauthorized.into());
    }

    handle_response(context, res)
  })
  .map_err(|err| err.error)
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

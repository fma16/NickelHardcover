use anyhow::{Context, Result};
use argh::FromArgs;
use itertools::Itertools;
use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};

use crate::{
  appcontext::AppContext,
  hardcover::{handle_response, transport_error},
};
use crate::{
  config::{BASE_URL, CLIENT_ID, VERSION},
  utils::ExpectedError,
};

#[derive(Serialize, Deserialize)]
struct OAuthToken {
  access_token: String,
  expires_in: i64,
  refresh_token: String,
  scope: String,
  token_type: String,
}

/// Get and store OAuth `access_token`, `refresh_token`, and expiration
#[derive(FromArgs, PartialEq)]
#[argh(subcommand, name = "oauth-set")]
pub struct OAuthSet {
  /// the device code returned by `oauth-request`
  #[argh(option)]
  device_code: String,
}

// `run` logs its arguments at INFO, which the hook forwards to syslog.
impl std::fmt::Debug for OAuthSet {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("OAuthSet").field("device_code", &"[redacted]").finish()
  }
}

pub fn run(context: &mut AppContext, args: &OAuthSet) -> Result<()> {
  log::info!("{VERSION} {args:?}");

  if let Err(err) = request_token(
    context,
    [
      ("client_id", CLIENT_ID),
      ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
      ("device_code", &args.device_code),
    ],
  ) {
    if matches!(err.downcast_ref(), Some(ExpectedError::Network(_))) {
      return Err(err);
    }
    return Err(ExpectedError::OAuth(format!("{:#}", err.chain().join("<br>> "))).into());
  }

  Ok(())
}

pub fn refresh_token(context: &mut AppContext) -> Result<()> {
  if let Err(err) = request_token(
    context,
    [
      ("client_id", CLIENT_ID),
      ("grant_type", "refresh_token"),
      ("refresh_token", &context.config.refresh_token.clone()),
    ],
  ) {
    if matches!(err.downcast_ref(), Some(ExpectedError::Network(_))) {
      return Err(err);
    }
    return Err(ExpectedError::OAuth(format!("{:#}", err.chain().join("<br>> "))).into());
  }

  Ok(())
}

pub fn request_token<I: IntoIterator<Item = (K, V)>, K: AsRef<str>, V: AsRef<str>>(
  context: &mut AppContext,
  body: I,
) -> Result<()> {
  let res = context
    .agent
    .post(format!("{}{}", BASE_URL, "/oauth2/token"))
    .send_form(body)
    .map_err(transport_error)
    .context("Failed to send OAuth device request")?;

  let json = handle_response(context, res).context("OAuth token request failed");

  match json {
    Ok(json) => {
      let value = serde_json::from_value::<OAuthToken>(json).context("Failed to deserialize OAuth token response")?;
      context.config.access_token = value.access_token;
      context.config.refresh_token = value.refresh_token;
      context.config.token_expires_at = Some(Timestamp::now() + SignedDuration::from_secs(value.expires_in));
      context.config.write()?;
      Ok(())
    }
    Err(e) if matches!(e.downcast_ref(), Some(ExpectedError::Network(_))) => Err(e),
    Err(e) => {
      context.config.access_token = String::new();
      context.config.refresh_token = String::new();
      context.config.token_expires_at = None;
      let _ = context.config.write();
      Err(e)
    }
  }
}

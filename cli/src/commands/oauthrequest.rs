use anyhow::{Context, Result};
use argh::FromArgs;
use itertools::Itertools;

use crate::config::{BASE_URL, CLIENT_ID, VERSION};
use crate::hardcover::{handle_response, retry_request, transport_error};
use crate::messages::Messages;
use crate::utils::send_msg;
use crate::{appcontext::AppContext, utils::ExpectedError};

/// Begin OAuth login flow
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "oauth-request")]
pub struct OAuthRequest {}

pub fn run(context: &mut AppContext, args: &OAuthRequest) -> Result<()> {
  log::info!("{VERSION} {args:?}");

  if let Err(err) = request_device_code(context) {
    if matches!(err.downcast_ref(), Some(ExpectedError::Network(_))) {
      return Err(err);
    }
    return Err(ExpectedError::OAuth(format!("{:#}", err.chain().join("<br>> "))).into());
  }

  Ok(())
}

fn request_device_code(context: &mut AppContext) -> Result<()> {
  let json = retry_request(|| {
    let res = context
      .agent
      .post(format!("{}{}", BASE_URL, "/oauth2/device"))
      .send_form([
        ("client_id", CLIENT_ID),
        (
          "scope",
          "read:catalog read:library read:journal read:me:content write:library write:reviews",
        ),
      ])
      .map_err(transport_error)
      .context("Failed to send OAuth device request")?;

    handle_response(context, res)
  })
  .context("OAuth device request failed")?;

  send_msg!(&Messages::OAuthDevice(
    serde_json::from_value(json).context("Failed to deserialize OAuth device response")?,
  ))
}

use anyhow::{Result, bail};
use argh::FromArgs;
use graphql_client::GraphQLQuery;
use jiff::Zoned;

use crate::appcontext::AppContext;
use crate::commands::getuserbook::get_book;
use crate::commands::setuserbook::{update_or_insert_user_book, update_user_book::UserBookUpdateInput};
use crate::commands::updatejournal::update_journal;
use crate::config::VERSION;
use crate::utils::{ExpectedError, GraphQLQueryExt, Percentage, normalize_identifiers};

#[derive(GraphQLQuery)]
#[graphql(
  schema_path = "src/graphql/schema.graphql",
  query_path = "src/graphql/mutations/updateread.graphql",
  custom_scalars_module = "crate::hardcover::scalars"
  response_derives = "Debug",
  variables_derives = "Debug"
)]
struct UpdateRead;

#[derive(GraphQLQuery)]
#[graphql(
  schema_path = "src/graphql/schema.graphql",
  query_path = "src/graphql/mutations/insertread.graphql",
  custom_scalars_module = "crate::hardcover::scalars"
  response_derives = "Debug",
  variables_derives = "Debug"
)]
struct InsertRead;

/// Update read percentage and create journal entries for annotations.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "update")]
pub struct Update {
  /// kobo book id or epub file path
  #[argh(option)]
  content_id: String,

  /// hardcover.app book or edition id
  #[argh(option)]
  linked_id: Option<i64>,

  /// read percentage (0-100)
  #[argh(option)]
  value: Percentage,

  /// synchronize only this service (hardcover or readwise), without changing config
  #[argh(option)]
  service: Option<String>,
}

pub fn run(context: &mut AppContext, args: &Update) -> Result<()> {
  match args.service.as_deref().unwrap_or(&context.config.provider) {
    "readwise" => crate::readwise::sync(context, &args.content_id, args.linked_id)
      .map_err(|error| ExpectedError::BookInfo(format!("Readwise Books: {error}")).into()),
    "hardcover" => sync_hardcover(context, args),
    "dual" if args.service.is_none() => {
      // --linked-id belongs to Hardcover in dual mode. Readwise resolves its own
      // IDs from the persisted sessions/cache, never from the Hardcover link.
      let hardcover = sync_hardcover(context, args);
      let readwise = crate::readwise::sync(context, "", None);
      match (hardcover, readwise) {
        (Ok(()), Ok(())) => Ok(()),
        (hardcover, readwise) => {
          let describe = |result: Result<()>| match result {
            Ok(()) => "OK".to_owned(),
            Err(error) => format!("{error:#}"),
          };
          bail!(ExpectedError::BookInfo(format!(
            "Hardcover: {}; Readwise Books: {}",
            describe(hardcover),
            describe(readwise)
          )))
        }
      }
    }
    _ => bail!(ExpectedError::InvalidConfig(
      "service must be hardcover or readwise".into()
    )),
  }
}

fn sync_hardcover(context: &mut AppContext, args: &Update) -> Result<()> {
  log::info!("{VERSION} {args:?}");

  let (linked_id, isbn) = normalize_identifiers(context, args.linked_id, Some(&args.content_id))?;
  let book = get_book(context, isbn, linked_id)?;
  let (user_book_id, user_read_id, started_at) = update_or_insert_user_book(
    context,
    &book,
    UserBookUpdateInput {
      status_id: Some(2),
      ..UserBookUpdateInput::default()
    },
  )?;
  let started_at = started_at.unwrap_or(Zoned::now().strftime("%F").to_string());

  let progress_pages = (book.pages as f64 * (args.value.0 / 100.0)).round() as i64;

  if let Some(user_read_id) = user_read_id {
    log::info!(
      "Update read `{user_read_id}` for edition `{}` to page `{progress_pages}`",
      book.edition_id
    );

    UpdateRead::send_request(
      context,
      update_read::Variables {
        id: user_read_id,
        progress_pages,
        edition_id: book.edition_id,
        started_at,
      },
    )?;
  } else {
    log::info!(
      "Insert new read for edition `{}` at page `{progress_pages}`",
      book.edition_id
    );

    InsertRead::send_request(
      context,
      insert_read::Variables {
        user_book_id,
        edition_id: book.edition_id,
        progress_pages,
        started_at,
      },
    )?;
  }

  if context.config.sync_annotations {
    update_journal(context, &args.content_id, &book)?;
  }

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::Config;

  #[test]
  fn dual_attempts_both_services_and_explicit_selection_does_not_change_config() {
    let mut context = AppContext {
      agent: ureq::Agent::new_with_defaults(),
      config: Config {
        provider: "dual".into(),
        ..Config::default()
      },
      rate_limit: 1,
      user: None,
    };
    let mut args = Update::from_args(
      &["update"],
      &["--content-id", "book", "--linked-id", "123", "--value", "54"],
    )
    .unwrap();
    let error = run(&mut context, &args).unwrap_err().to_string();
    assert!(error.contains("Hardcover: Unauthorized"));
    assert!(error.contains("Readwise Books:") && error.contains("readwise_token"));
    args.service = Some("hardcover".into());
    assert!(matches!(
      run(&mut context, &args).unwrap_err().downcast_ref(),
      Some(ExpectedError::Unauthorized)
    ));
    args.service = Some("readwise".into());
    let error = run(&mut context, &args).unwrap_err().to_string();
    assert!(error.contains("readwise_token") && !error.contains("Unauthorized"));
    assert_eq!(context.config.provider, "dual");
  }
}

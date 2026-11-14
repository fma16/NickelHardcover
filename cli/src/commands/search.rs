use anyhow::{Context, Result};
use argh::FromArgs;
use graphql_client::GraphQLQuery;
use serde_json::Value;

use crate::appcontext::AppContext;
use crate::config::VERSION;
use crate::messages::{Messages, SearchPages, SearchResult, Series};
use crate::utils::{GraphQLQueryExt, send_msg};

#[derive(GraphQLQuery)]
#[graphql(
  schema_path = "src/graphql/schema.graphql",
  query_path = "src/graphql/queries/searchbooks.graphql",
  custom_scalars_module = "crate::hardcover::scalars"
  response_derives = "Debug",
  variables_derives = "Debug"
)]
struct SearchBooks;

/// Search for books.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "search")]
pub struct Search {
  /// how many results per page
  #[argh(option)]
  limit: u64,

  /// which page
  #[argh(option)]
  page: i64,

  /// search query
  #[argh(option)]
  query: String,
}

pub fn run(context: &mut AppContext, args: Search) -> Result<()> {
  if context.config.provider == "readwise" {
    return crate::readwise::search(context, &args.query, args.limit, args.page);
  }
  log::info!("{VERSION} {args:?}");

  let res = SearchBooks::send_request(
    context,
    search_books::Variables {
      query: args.query,
      limit: args.limit as i64,
      page: args.page,
    },
  )?
  .search
  .context("Failed to find field <i>search</i> in Hardcover.app results")?
  .results
  .context("Failed to find field <i>results</i> in Hardcover.app results")?;

  send_msg!(&Messages::SearchPages(parse_search_pages(&res, args.limit)?))
}

fn parse_search_pages(res: &Value, limit: u64) -> Result<SearchPages> {
  let results = res
    .get("hits")
    .and_then(Value::as_array)
    .context("Failed to find field <i>hits</i> in Hardcover.app results")?
    .iter()
    .filter_map(|hit| hit.get("document"))
    .map(|doc| SearchResult {
      authors: doc
        .get("contributions")
        .and_then(Value::as_array)
        .iter()
        .flat_map(|contributions| {
          contributions
            .iter()
            .filter_map(|item| match item.get("contribution").and_then(Value::as_str) {
              Some("Author") | None => item
                .get("author")
                .and_then(|author| author.get("name"))
                .and_then(Value::as_str)
                .map(str::to_string),
              _ => None,
            })
        })
        .collect::<Vec<_>>(),
      id: doc.get("id").and_then(Value::as_str).map(str::to_string),
      image: doc
        .get("image")
        .and_then(|image| image.get("url"))
        .and_then(Value::as_str)
        .map(str::to_string),
      rating: doc.get("rating").and_then(Value::as_f64),
      release_year: doc.get("release_year").and_then(Value::as_i64),
      series: doc.get("featured_series").map(|featured_series| Series {
        name: featured_series
          .get("series")
          .and_then(|s| s.get("name"))
          .and_then(Value::as_str)
          .map(str::to_string),
        position: featured_series.get("position").and_then(Value::as_f64),
        primary_books_count: featured_series
          .get("series")
          .and_then(|series| series.get("primary_books_count"))
          .and_then(Value::as_u64),
      }),
      title: doc.get("title").and_then(Value::as_str).map(str::to_string),
      users_count: doc.get("users_count").and_then(Value::as_u64),
    })
    .collect::<Vec<_>>();

  Ok(SearchPages {
    results,
    page: res.get("page").and_then(Value::as_u64).unwrap_or(0),
    total: match res.get("found").and_then(Value::as_u64) {
      Some(0) => 0,
      Some(n) => n.div_ceil(limit).max(1),
      None => 1,
    },
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn search_preserves_authors_and_fractional_metadata() {
    let result = parse_search_pages(
      &json!({
        "hits": [{"document": {
          "id": "123", "title": "A book", "rating": 4.31,
          "contributions": [
            {"contribution": "Author", "author": {"name": "Named author"}},
            {"contribution": null, "author": {"name": "Legacy author"}},
            {"author": {"name": "Unspecified author"}},
            {"contribution": "Illustrator", "author": {"name": "Illustrator"}}
          ],
          "featured_series": {"position": 1.5, "series": {"name": "Series", "primary_books_count": 4}}
        }}, {"document": {"title": "No metadata"}}, {"document": {
          "rating": 4, "featured_series": {"position": 1.0}
        }}],
        "page": 2, "found": 7
      }),
      3,
    )
    .unwrap();
    let output = serde_json::to_value(result).unwrap();
    assert_eq!(
      output["results"][0]["authors"],
      json!(["Named author", "Legacy author", "Unspecified author"])
    );
    assert_eq!(output["results"][0]["rating"], json!(4.31));
    assert_eq!(output["results"][0]["series"]["position"], json!(1.5));
    assert_eq!(output["results"][1]["rating"], Value::Null);
    assert_eq!(output["results"][1]["authors"], json!([]));
    assert_eq!(output["results"][2]["rating"].as_f64(), Some(4.0));
    assert_eq!(output["results"][2]["series"]["position"].as_f64(), Some(1.0));
    assert_eq!((output["page"].as_u64(), output["total"].as_u64()), (Some(2), Some(3)));
  }
}

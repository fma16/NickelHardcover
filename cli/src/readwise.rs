//! Readwise Books v1. Session POSTs are deliberately never retried after an
//! ambiguous response: the API has no idempotency key or duplicate detection.
use std::{fs, path::PathBuf, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use jiff::Timestamp;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ureq::Agent;

use crate::{
  appcontext::AppContext,
  config::Config,
  messages::{Messages, SearchPages, SearchResult, User, UserBook},
  utils::{ExpectedError, normalize_identifiers, send_msg},
};

const BASE: &str = "https://readwise.io/books/api/v1";

#[cfg(test)]
mod tests;

pub struct Client {
  agent: Agent,
  token: String,
  base: String,
  directory: PathBuf,
}

#[derive(Debug)]
struct ApiError {
  message: String,
  // Only an explicit rejection proves that a session was not created.
  rejected: bool,
  retryable: bool,
}

impl std::fmt::Display for ApiError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(&self.message)
  }
}
impl std::error::Error for ApiError {}

#[derive(Deserialize, Serialize)]
struct Session {
  content_id: String,
  title: String,
  author: String,
  // Absent from sessions queued before this field existed or when Nickel does not export the name.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  device_model: Option<String>,
  linked_id: Option<i64>,
  book_id: Option<i64>,
  started_at: Timestamp,
  ended_at: Timestamp,
  percent_at_start: f64,
  percent_at_end: f64,
  publish_to_feed: bool,
}

impl Client {
  fn new(context: &AppContext) -> Result<Self> {
    ensure!(!context.config.readwise_token.trim().is_empty(), ExpectedError::BookInfo(
      "Set readwise_token in .adds/NickelHardcover/config.ini using https://readwise.io/access_token, then reopen the book.".into()));
    Ok(Self {
      agent: Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .timeout_global(Some(Duration::from_secs(30)))
        .user_agent("Bookwise-Nickel/0.1")
        .build()
        .into(),
      token: context.config.readwise_token.trim().into(),
      base: BASE.into(),
      directory: Config::get_dir()?,
    })
  }

  fn request(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value, ApiError> {
    let retry_path = self.directory.join("readwise-retry-at");
    if let Ok(value) = fs::read_to_string(&retry_path)
      && let Ok(until) = value.parse::<i64>()
      && until > Timestamp::now().as_second()
    {
      return Err(ApiError {
        message: "Readwise rate limit: wait before syncing again.".into(),
        rejected: true,
        retryable: true,
      });
    }
    let url = format!("{}{path}", self.base);
    let auth = format!("Token {}", self.token);
    let response = match method {
      "GET" => self.agent.get(&url).header("Authorization", &auth).call(),
      "POST" => self
        .agent
        .post(&url)
        .header("Authorization", &auth)
        .send_json(body.unwrap()),
      "PATCH" => self
        .agent
        .patch(&url)
        .header("Authorization", &auth)
        .send_json(body.unwrap()),
      _ => unreachable!(),
    };
    // Never include request/header debug output, which could contain the token.
    let mut response = response.map_err(|_| ApiError {
      message: "Readwise network failure. A session whose delivery is uncertain will not be sent again automatically."
        .into(),
      rejected: false,
      retryable: false,
    })?;
    let status = response.status().as_u16();
    if status == 429 {
      let seconds = response
        .headers()
        .get("retry-after")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(60)
        .max(1);
      let until = Timestamp::now().as_second().saturating_add(seconds);
      fs::write(retry_path, until.to_string()).map_err(|_| ApiError {
        message: "Cannot save Readwise rate-limit deadline.".into(),
        rejected: true,
        retryable: false,
      })?;
    }
    if status != 200 {
      return Err(ApiError {
        message: match status {
          401 => "Readwise token is invalid. Update readwise_token in config.ini.".into(),
          403 => "Readwise access denied. Check that your account has Readwise Books.".into(),
          429 => "Readwise rate limit reached. Retry after the server's waiting period.".into(),
          _ => format!("Readwise returned HTTP {status}."),
        },
        rejected: (400..500).contains(&status),
        retryable: matches!(status, 401 | 403 | 429),
      });
    }
    response.body_mut().read_json().map_err(|_| ApiError {
      message: "Invalid Readwise response; session delivery may be uncertain.".into(),
      rejected: false,
      retryable: false,
    })
  }

  fn batch(&self, method: &str, path: &str, item: Value) -> Result<Value, ApiError> {
    let response = self.request(
      method,
      path,
      Some(Value::Object(
        [("items".into(), Value::Array(vec![item]))].into_iter().collect(),
      )),
    )?;
    let results = response["results"]
      .as_array()
      .filter(|r| r.len() == 1)
      .ok_or_else(|| ApiError {
        message: "Invalid Readwise batch response; session delivery may be uncertain.".into(),
        rejected: false,
        retryable: false,
      })?;
    let result = &results[0];
    if result["status"] == "error" {
      let code = result["error"]["code"].as_str().unwrap_or("unknown");
      return Err(ApiError {
        // Do not render remote message text as HTML in Nickel dialogs.
        message: format!("Readwise rejected the item ({code})."),
        rejected: true,
        retryable: matches!(code, "rate_limited" | "unauthenticated" | "forbidden"),
      });
    }
    let valid = if method == "PATCH" {
      result["status"] == "updated"
    } else {
      result["status"] == "created" || result["status"] == "existing"
    };
    if !valid {
      return Err(ApiError {
        message: "Unknown Readwise batch outcome.".into(),
        rejected: false,
        retryable: false,
      });
    }
    Ok(result.clone())
  }

  fn book(&self, id: i64) -> Result<Value> {
    ensure!(id > 0, "Invalid Readwise library book ID");
    let value = self.request("GET", &format!("/books?ids={id}"), None)?;
    value["results"]
      .as_array()
      .and_then(|books| books.iter().find(|b| b["id"] == id))
      .cloned()
      .ok_or_else(|| {
        ExpectedError::BookNotFound("Readwise library book not found. Unlink it and select another book.".into()).into()
      })
  }

  fn ensure_book(
    &self,
    context: &mut AppContext,
    content_id: &str,
    linked_id: Option<i64>,
    title: &str,
    author: &str,
  ) -> Result<i64> {
    // Cache even unmatched books: POST /books only deduplicates catalog matches.
    let mut db = Connection::open(self.directory.join("readwise.sqlite"))?;
    db.busy_timeout(Duration::from_secs(35))?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS books (content_id TEXT PRIMARY KEY, book_id INTEGER NOT NULL)")?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some(id) = linked_id {
      self.book(id)?;
      if !content_id.is_empty() {
        tx.execute(
          "INSERT OR REPLACE INTO books VALUES (?1, ?2)",
          rusqlite::params![content_id, id],
        )?;
      }
      tx.commit()?;
      return Ok(id);
    }
    if let Some(id) = tx
      .query_row("SELECT book_id FROM books WHERE content_id=?1", [content_id], |r| {
        r.get(0)
      })
      .optional()?
    {
      ensure!(id > 0, ExpectedError::BookNotFound("A previous book creation had an uncertain outcome. Find the book in Readwise and link it manually before retrying.".into()));
      self.book(id)?;
      return Ok(id);
    }
    let product = match normalize_identifiers(context, None, Some(content_id)) {
      Ok((_, identifiers)) if !identifiers.is_empty() => {
        let id = identifiers.iter().find(|s| s.len() == 13).unwrap_or(&identifiers[0]);
        if id.chars().all(|c| c.is_ascii_digit()) || id.ends_with('X') {
          json!({"isbn": id})
        } else {
          json!({"asin": id})
        }
      }
      _ => {
        ensure!(
          !title.trim().is_empty() && !author.trim().is_empty(),
          ExpectedError::BookNotFound(
            "No ISBN or title/author available. Add the book to Readwise Books, then use Manually link book.".into()
          )
        );
        json!({"title": title, "author": author})
      }
    };
    tx.execute("INSERT INTO books VALUES (?1, -1)", [content_id])?;
    tx.commit()?;
    let result = match self.batch(
      "POST",
      "/books",
      json!({"product": product, "status": "currently_reading"}),
    ) {
      Ok(result) => result,
      Err(error) => {
        if error.rejected {
          db.execute("DELETE FROM books WHERE content_id=?1", [content_id])?;
        }
        return Err(error.into());
      }
    };
    let id = result["book_id"]
      .as_i64()
      .filter(|id| *id > 0)
      .context("Readwise returned no book_id")?;
    db.execute(
      "UPDATE books SET book_id=?2 WHERE content_id=?1",
      rusqlite::params![content_id, id],
    )?;
    Ok(id)
  }

  fn flush(&self, context: &mut AppContext) -> Result<()> {
    let lock = fs::File::options()
      .write(true)
      .create(true)
      .truncate(false)
      .open(self.directory.join("readwise-sync.lock"))?;
    lock.try_lock().context("Another Readwise sync is already running")?;
    let spool = self.directory.join("readwise-sessions");
    fs::create_dir_all(&spool)?;
    let mut paths = fs::read_dir(&spool)?
      .map(|entry| entry.map(|e| e.path()))
      .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort(); // UTC timestamp prefixes keep older progress before newer progress.
    // Stop rather than sending newer progress over an unresolved older session.
    ensure!(
      !paths
        .iter()
        .any(|p| matches!(p.extension().and_then(|s| s.to_str()), Some("sending" | "failed"))),
      "A Readwise session needs attention in readwise-sessions (.sending or .failed). See README before retrying."
    );
    for path in paths.iter().filter(|p| p.extension().is_some_and(|e| e == "json")) {
      let mut session: Session = serde_json::from_slice(&fs::read(path)?)?;
      ensure!(
        session.ended_at > session.started_at
          && session.ended_at <= Timestamp::now()
          && (0.0..=1.0).contains(&session.percent_at_start)
          && (0.0..=1.0).contains(&session.percent_at_end)
          && session.percent_at_end > session.percent_at_start,
        "Invalid queued Readwise session"
      );
      let id = self.ensure_book(
        context,
        &session.content_id,
        session.linked_id,
        &session.title,
        &session.author,
      )?;
      let sending = path.with_extension("sending");
      // Atomic claim prevents two sync processes from POSTing this same file.
      fs::rename(path, &sending).context("Cannot claim queued session; another sync may be running")?;
      session.book_id = Some(id);
      fs::write(&sending, serde_json::to_vec(&session)?)?;
      fs::File::open(&sending)?.sync_all()?;
      fs::File::open(&spool)?.sync_all()?;
      let mut device = json!({"type": "E-Reader", "vendor": "Kobo"});
      // The feed shows only the model, so keep Nickel's full name such as "Kobo Libra Colour".
      // Readwise rejects a model outside 1 to 100 characters.
      if let Some(model) = session.device_model.as_deref().map(str::trim)
        && (1..=100).contains(&model.chars().count())
      {
        device["model"] = model.into();
      }
      let outcome = self.batch(
        "POST",
        "/sessions",
        json!({
          "book_id": id, "started_at": session.started_at, "ended_at": session.ended_at,
          "format": "ebook", "percent_at_start": session.percent_at_start,
          "percent_at_end": session.percent_at_end, "publish_to_feed": session.publish_to_feed,
          "device": device
        }),
      );
      match outcome {
        Ok(result) if result["session_id"].as_i64().is_some_and(|id| id > 0) => {
          // Persist the acknowledgement before any subsequent book PATCH.
          fs::rename(&sending, path.with_extension("sent"))?;
          fs::File::open(&spool)?.sync_all()?;
        }
        Ok(_) => bail!("Readwise returned no session_id. Delivery is uncertain; see README."),
        Err(error) => {
          if error.rejected {
            fs::rename(
              &sending,
              if error.retryable {
                path.clone()
              } else {
                path.with_extension("failed")
              },
            )?;
          }
          return Err(error.into());
        }
      }
    }
    // A failed shelf update must not cause an already accepted session to replay.
    let mut sent = fs::read_dir(&spool)?
      .map(|e| e.map(|e| e.path()))
      .collect::<std::io::Result<Vec<_>>>()?;
    sent.sort();
    for path in sent.iter().filter(|p| p.extension().is_some_and(|e| e == "sent")) {
      let session: Session = serde_json::from_slice(&fs::read(path)?)?;
      let id = session
        .book_id
        .context("Acknowledged session has no Readwise book ID")?;
      // 1.0 is the exact API boundary; rounding near-complete books up is wrong.
      #[allow(clippy::float_cmp)]
      let finished = session.percent_at_end == 1.0;
      let mut change = json!({"id": id, "status": if finished { "finished" } else { "currently_reading" }});
      if finished {
        change["finished_reading_at"] = json!(session.ended_at);
      }
      self.batch("PATCH", "/books", change)?;
      fs::remove_file(path)?;
    }
    Ok(())
  }
}

pub fn sync(context: &mut AppContext, content_id: &str, linked_id: Option<i64>) -> Result<()> {
  let client = Client::new(context)?;
  if linked_id.is_some() {
    client.ensure_book(context, content_id, linked_id, "", "")?;
  }
  client.flush(context)
}

pub fn get_user(context: &AppContext) -> Result<()> {
  Client::new(context)?.request("GET", "/books?limit=1", None)?;
  send_msg!(&Messages::User(User {
    id: 0,
    username: Some("Readwise Books".into()),
    account_privacy_setting_id: 0,
    account_privacy_setting: "private".into()
  }))
}

fn resolve(context: &mut AppContext, content_id: Option<&str>, linked_id: Option<i64>) -> Result<(Client, i64)> {
  let client = Client::new(context)?;
  let id = client.ensure_book(context, content_id.unwrap_or(""), linked_id, "", "")?;
  Ok((client, id))
}

pub fn get_book(context: &mut AppContext, content_id: Option<&str>, linked_id: Option<i64>) -> Result<()> {
  let (client, id) = resolve(context, content_id, linked_id)?;
  let book = client.book(id)?;
  send_msg!(&Messages::UserBook(UserBook {
    user_book_id: id,
    status_id: match book["status"].as_str() {
      Some("want_to_read") => 1,
      Some("currently_reading") => 2,
      Some("finished") => 3,
      Some("did_not_finish") => 5,
      _ => 0,
    },
    rating: book["rating"].as_f64(),
    review_raw: book["review"].as_str().map(str::to_string),
    reviewed_at: None,
    review_has_spoilers: false,
    sponsored_review: false,
  }))
}

pub fn set_book(
  context: &mut AppContext,
  content_id: Option<&str>,
  linked_id: Option<i64>,
  status: Option<i64>,
  rating: Option<f64>,
  text: Option<String>,
) -> Result<()> {
  let mut item = json!({});
  if let Some(status) = status {
    item["status"] = json!(match status {
      1 => "want_to_read",
      2 => "currently_reading",
      3 => "finished",
      5 => "did_not_finish",
      _ => bail!("Readwise Books has no Paused shelf"),
    });
  }
  if let Some(rating) = rating {
    ensure!(
      (0.25..=5.0).contains(&rating) && (rating * 4.0).fract() == 0.0,
      "Rating must be 0.25–5 in quarter-star steps"
    );
    item["rating"] = json!(rating);
  }
  if let Some(text) = text {
    item["review"] = if text.is_empty() { Value::Null } else { json!(text) };
  }
  let (client, id) = resolve(context, content_id, linked_id)?;
  item["id"] = json!(id);
  client.batch("PATCH", "/books", item)?;
  Ok(())
}

pub fn search(context: &AppContext, query: &str, limit: u64, page: i64) -> Result<()> {
  ensure!((1..=1000).contains(&limit) && page > 0, "Invalid search pagination");
  let page = u64::try_from(page)?;
  let client = Client::new(context)?;
  let mut path = "/books?limit=1000".to_string();
  let mut books = Vec::new();
  loop {
    let value = client.request("GET", &path, None)?;
    books.extend(
      value["results"]
        .as_array()
        .context("Missing books results")?
        .iter()
        .cloned(),
    );
    let Some(next) = value["next"].as_str() else { break };
    // Only follow pagination on this API, never forward credentials elsewhere.
    path = next
      .strip_prefix(&format!("{}/", client.base))
      .filter(|p| p.starts_with("books?"))
      .map(|p| format!("/{p}"))
      .context("Untrusted Readwise pagination URL")?;
  }
  let query = query.to_lowercase();
  let matching: Vec<_> = books
    .iter()
    .filter(|b| {
      b["status"] != "deleted"
        && format!(
          "{} {}",
          b["title"].as_str().unwrap_or(""),
          b["author"].as_str().unwrap_or("")
        )
        .to_lowercase()
        .contains(&query)
    })
    .collect();
  let total = (matching.len() as u64).div_ceil(limit);
  let results = matching
    .into_iter()
    .skip((page as usize - 1).saturating_mul(limit as usize))
    .take(limit as usize)
    .map(|b| SearchResult {
      id: b["id"].as_i64().map(|id| id.to_string()),
      title: b["title"].as_str().map(str::to_string),
      authors: b["author"].as_str().map(str::to_string).into_iter().collect(),
      image: b["product"]["cover_url"].as_str().map(str::to_string),
      rating: None,
      release_year: None,
      series: None,
      users_count: None,
    })
    .collect();
  send_msg!(&Messages::SearchPages(SearchPages { results, page, total }))
}

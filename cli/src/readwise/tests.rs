use super::*;
use std::{
  io::{BufRead, BufReader, Read, Write},
  net::TcpListener,
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
  },
  thread,
};

struct Server {
  base: String,
  requests: Arc<Mutex<Vec<(String, Value)>>>,
  stop: Arc<AtomicBool>,
  thread: Option<thread::JoinHandle<()>>,
}

impl Server {
  fn new(responses: Vec<(u16, String)>) -> Self {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/books/api/v1", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let output = requests.clone();
    let shutdown = stop.clone();
    let thread = thread::spawn(move || {
      let mut responses = responses.into_iter();
      while !shutdown.load(Ordering::SeqCst) {
        let Ok((mut stream, _)) = listener.accept() else {
          thread::sleep(Duration::from_millis(2));
          continue;
        };
        stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request = String::new();
        reader.read_line(&mut request).unwrap();
        let mut length = 0;
        let mut authorized = false;
        loop {
          let mut header = String::new();
          reader.read_line(&mut header).unwrap();
          if header == "\r\n" {
            break;
          }
          if let Some((key, value)) = header.split_once(':') {
            if key.eq_ignore_ascii_case("content-length") {
              length = value.trim().parse().unwrap();
            }
            if key.eq_ignore_ascii_case("authorization") {
              authorized = value.trim() == "Token test-token";
            }
          }
        }
        assert!(authorized);
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        let body = if body.is_empty() {
          Value::Null
        } else {
          serde_json::from_slice(&body).unwrap()
        };
        output.lock().unwrap().push((request.trim().into(), body));
        let (status, body) = responses.next().expect("Unexpected request (possible duplicate write)");
        if status == 0 {
          continue;
        } // Server consumed the request but lost its reply.
        write!(stream, "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nRetry-After: 60\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
      }
    });
    Self {
      base,
      requests,
      stop,
      thread: Some(thread),
    }
  }

  fn requests(&self) -> Vec<(String, Value)> {
    self.requests.lock().unwrap().clone()
  }
}

impl Drop for Server {
  fn drop(&mut self) {
    self.stop.store(true, Ordering::SeqCst);
    self.thread.take().unwrap().join().unwrap();
  }
}

static COUNTER: AtomicU64 = AtomicU64::new(0);
struct Fixture {
  client: Client,
  context: AppContext,
}
impl Fixture {
  fn new(server: &Server) -> Self {
    let directory = std::env::temp_dir().join(format!(
      "bookwise-test-{}-{}",
      std::process::id(),
      COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(directory.join("readwise-sessions")).unwrap();
    let context = AppContext {
      agent: Agent::new_with_defaults(),
      config: Config {
        provider: "readwise".into(),
        readwise_token: "test-token".into(),
        ..Config::default()
      },
      rate_limit: 1,
      user: None,
    };
    let mut client = Client::new(&context).unwrap();
    client.base.clone_from(&server.base);
    client.directory = directory;
    Self { client, context }
  }

  fn queue(&self, name: &str, end: f64) -> PathBuf {
    let path = self
      .client
      .directory
      .join("readwise-sessions")
      .join(format!("{name}.json"));
    fs::write(
      &path,
      json!({
        "content_id": "file:///missing-test.epub", "title": "Unmatched book", "author": "Test Author",
        "started_at": "2026-09-10T20:14:05Z", "ended_at": "2026-09-10T20:58:35Z",
        "percent_at_start": 0.412, "percent_at_end": end,
        "publish_to_feed": self.context.config.readwise_publish_to_feed
      })
      .to_string(),
    )
    .unwrap();
    path
  }

  fn sync(&mut self) -> Result<()> {
    self.client.flush(&mut self.context)
  }
}
impl Drop for Fixture {
  fn drop(&mut self) {
    fs::remove_dir_all(&self.client.directory).unwrap();
  }
}

fn created_book() -> (u16, String) {
  (
    200,
    json!({"results": [{"status": "created", "matched_by": null, "book_id": 4790033, "error": null}]}).to_string(),
  )
}
fn created_session() -> (u16, String) {
  (
    200,
    json!({"results": [{"status": "created", "session_id": 90211876, "error": null}]}).to_string(),
  )
}
fn updated_book() -> (u16, String) {
  (
    200,
    json!({"results": [{"status": "updated", "id": 4790033, "error": null}]}).to_string(),
  )
}

fn reading_book() -> (u16, String) {
  (
    200,
    json!({"results": [{"id": 4790033, "status": "currently_reading"}]}).to_string(),
  )
}

#[test]
fn future_time_rejection_keeps_original_session_retryable() {
  // The device believes the session just ended; the server is 90s behind it.
  let device_now = Timestamp::now();
  let server_now = device_now.as_second() - 90;
  // Wording observed from the real service for an end time about 99s ahead.
  let error = json!({"code": "invalid_value",
    "message": "Invalid value for body: Value error, times must not be in the future",
    "details": [{"location": "body", "field": "items.0.body", "code": "invalid_value",
      "message": "Value error, times must not be in the future"}]});
  for response in [
    (400, json!({"error": error}).to_string()),
    (
      200,
      json!({"results": [{"status": "error", "error": error}]}).to_string(),
    ),
  ] {
    let server = Server::new(vec![
      created_book(),
      response,
      reading_book(),
      created_session(),
      reading_book(),
      updated_book(),
    ]);
    let mut fixture = Fixture::new(&server);
    let path = fixture.queue("01", 0.447);
    let mut session: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    session["started_at"] = json!(Timestamp::from_second(device_now.as_second() - 120).unwrap());
    session["ended_at"] = json!(device_now);
    fs::write(&path, session.to_string()).unwrap();
    let message = fixture.sync().unwrap_err().to_string();
    let posted = &server.requests()[1].1["items"][0];
    assert!(
      posted["ended_at"]
        .as_str()
        .unwrap()
        .parse::<Timestamp>()
        .unwrap()
        .as_second()
        > server_now
    );
    assert!(
      path.exists(),
      "explicit future rejection must remain retryable: {message}"
    );
    assert!(!path.with_extension("failed").exists());
    let retained: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(retained["started_at"], session["started_at"]);
    assert_eq!(retained["ended_at"], session["ended_at"]);
    // The advice leads; the request diagnostic follows without repeating the reason.
    assert!(
      message.starts_with("Readwise refused a session that ends in the future"),
      "{message}"
    );
    assert!(message.contains("Check the Kobo clock"), "{message}");
    assert!(message.contains("POST /sessions"), "{message}");
    assert_eq!(
      message.matches("times must not be in the future").count(),
      1,
      "{message}"
    );
    // A later server response accepts the unchanged times. Only the explicit
    // rejection is retried; an accepted session is never posted a third time.
    fixture.sync().unwrap();
    fixture.sync().unwrap();
    let posts: Vec<_> = server
      .requests()
      .into_iter()
      .filter(|r| r.0 == "POST /books/api/v1/sessions HTTP/1.1")
      .collect();
    assert_eq!(posts.len(), 2);
    assert_eq!(posts[0].1, posts[1].1);
    assert!(!path.exists());
  }
  // Correcting the local clock must not quarantine an otherwise valid future
  // interval. No HTTP request is needed until its recorded end time passes.
  let server = Server::new(vec![]);
  let mut fixture = Fixture::new(&server);
  let path = fixture.queue("01", 0.447);
  let mut session: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
  session["ended_at"] = json!(Timestamp::from_second(Timestamp::now().as_second() + 90).unwrap());
  let original = session.to_string();
  fs::write(&path, &original).unwrap();
  let message = fixture.sync().unwrap_err().to_string();
  assert!(message.contains("Check the Kobo clock"), "{message}");
  // The wait is given as a readable UTC time, not a raw RFC 3339 timestamp.
  let end = session["ended_at"].as_str().unwrap().parse::<Timestamp>().unwrap();
  let readable = end.strftime("%Y-%m-%d %H:%M:%S UTC").to_string();
  assert!(message.contains(&readable), "{message}");
  assert_eq!(fs::read_to_string(path).unwrap(), original);
  assert!(server.requests().is_empty());
}

#[test]
fn corrupt_and_invalid_sessions_are_named_and_marked_without_skipping_them() {
  for corrupt in [false, true] {
    let server = Server::new(vec![]);
    let mut fixture = Fixture::new(&server);
    let path = fixture.queue("20261112000000000-{corrupt}", 0.412);
    if corrupt {
      fs::write(&path, "{\"content_id\":\"kobo-hobbit\",\"title\":\"The Ho").unwrap();
    }
    let original = fs::read(&path).unwrap();
    let newer = fixture.queue("20261113000000000-valid", 0.8);
    let message = fixture.sync().unwrap_err().to_string();
    // Named as seen over USB, with the reason, not by its absolute path.
    let name = "readwise-sessions/20261112000000000-{corrupt}.failed";
    assert!(message.contains(&format!("session {name} is invalid")), "{message}");
    assert!(!message.contains(path.parent().unwrap().to_str().unwrap()), "{message}");
    let reason = if corrupt {
      "the file is incomplete or damaged"
    } else {
      "it records no reading progress"
    };
    assert!(message.contains(reason), "{message}");
    assert!(!path.exists());
    assert_eq!(fs::read(path.with_extension("failed")).unwrap(), original);
    assert!(newer.exists());
    let message = fixture.sync().unwrap_err().to_string();
    assert!(message.contains("Readwise uploads are paused"), "{message}");
    assert!(message.contains(name), "{message}");
    assert!(server.requests().is_empty());
  }
}

#[test]
fn deleted_books_are_not_restored_by_pending_or_acknowledged_sessions() {
  for acknowledged in [true, false] {
    let mut responses = vec![
      (
        200,
        json!({"results": [{"id": 4790033, "status": "deleted"}]}).to_string(),
      ),
      reading_book(),
    ];
    if !acknowledged {
      responses.extend([created_session(), reading_book()]);
    }
    responses.push(updated_book());
    let server = Server::new(responses);
    let mut fixture = Fixture::new(&server);
    let path = fixture.queue("01", 0.8);
    let mut session: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    session["linked_id"] = json!(4790033);
    session["book_id"] = json!(4790033);
    fs::write(&path, session.to_string()).unwrap();
    let path = if acknowledged {
      let sent = path.with_extension("sent");
      fs::rename(path, &sent).unwrap();
      sent
    } else {
      path
    };
    let result = fixture.sync();
    let requests = server.requests();
    assert_eq!(
      requests.len(),
      1,
      "no session POST or shelf PATCH for a deleted book: {requests:?}"
    );
    assert!(
      requests[0].0.starts_with("GET "),
      "must check shelf before writing: {requests:?}"
    );
    assert!(result.unwrap_err().to_string().contains("deleted"));
    assert!(path.exists());
    // Once the user restores the shelf, resume without replaying a .sent.
    fixture.sync().unwrap();
    assert!(!path.exists());
    let posts = server
      .requests()
      .iter()
      .filter(|r| r.0 == "POST /books/api/v1/sessions HTTP/1.1")
      .count();
    assert_eq!(posts, usize::from(!acknowledged));
  }
}

#[test]
fn books_and_highlights_rate_limits_are_independent_and_persistent() {
  let request = |client: &Client, path: &str| {
    if path == HIGHLIGHTS {
      client.request(
        "POST",
        path,
        Some(json!({"highlights": [{"text": "Test highlight", "title": "Test book"}]})),
      )
    } else {
      client.request("GET", path, None)
    }
  };
  for (limited, other) in [(HIGHLIGHTS, "/books?limit=1"), ("/books?limit=1", HIGHLIGHTS)] {
    let server = Server::new(vec![(429, "{}".into()), (200, "{}".into())]);
    let fixture = Fixture::new(&server);
    // Preserve an outstanding Retry-After across upgrades; its API is unknown.
    let legacy = fixture.client.directory.join("readwise-retry-at");
    fs::write(&legacy, (Timestamp::now().as_second() + 60).to_string()).unwrap();
    assert!(request(&fixture.client, limited).is_err());
    assert!(request(&fixture.client, other).is_err());
    assert!(server.requests().is_empty());
    fs::write(&legacy, "0").unwrap();
    assert!(request(&fixture.client, limited).is_err());
    // Recreate the client: the deadline must survive separate CLI processes.
    let mut next = Client::new(&fixture.context).unwrap();
    next.base.clone_from(&server.base);
    next.directory.clone_from(&fixture.client.directory);
    assert!(request(&next, other).is_ok(), "429 on {limited} blocked {other}");
    assert!(request(&next, limited).is_err());
    assert_eq!(server.requests().len(), 2);
  }
}

#[test]
fn readwise_update_errors_do_not_offer_hardcover_linking_or_hide_auth_failures() {
  use argh::FromArgs;
  let server = Server::new(vec![(200, json!({"results": []}).to_string())]);
  let mut fixture = Fixture::new(&server);
  let error = fixture.client.book(4790033).unwrap_err();
  let mapped = sync_error(&error, None);
  assert_eq!(
    mapped.downcast_ref::<ExpectedError>().unwrap().as_ref(),
    "ReadwiseBookNotFound"
  );
  let message = mapped.to_string();
  // Nickel already titles this dialog "Readwise Books".
  assert!(
    message.starts_with("Readwise library book 4790033 not found"),
    "{message}"
  );
  fixture.context.config.provider = "dual".into();
  fixture.context.config.readwise_token.clear();
  let args = crate::commands::update::Update::from_args(
    &["update"],
    &["--content-id", "book", "--linked-id", "123", "--value", "54"],
  )
  .unwrap();
  let error = crate::commands::update::run(&mut fixture.context, &args).unwrap_err();
  assert_eq!(error.downcast_ref::<ExpectedError>().unwrap().as_ref(), "ReadwiseAuth");
  assert!(error.to_string().contains("Hardcover: Unauthorized"));
  assert!(error.to_string().contains("readwise_token"));
  assert!(message.contains("provider=readwise"), "{message}");
  assert!(!message.contains("Unlink it"));
  let args = crate::commands::update::Update::from_args(
    &["update"],
    &["--service", "readwise", "--content-id", "book", "--value", "54"],
  )
  .unwrap();
  let error = crate::commands::update::run(&mut fixture.context, &args).unwrap_err();
  assert_eq!(error.downcast_ref::<ExpectedError>().unwrap().as_ref(), "ReadwiseAuth");
  assert!(!error.to_string().contains("Hardcover:"));
  // Exercise real HTTP failures through the same mapper used by update, not
  // just the missing-token check. Retain both services' messages in dual mode.
  for (status, code) in [
    (401, "ReadwiseAuth"),
    (403, "ReadwiseAuth"),
    (429, "ReadwiseSync"),
    (500, "ReadwiseSync"),
  ] {
    let server = Server::new(vec![created_book(), (status, "{}".into())]);
    let mut fixture = Fixture::new(&server);
    fixture.queue("01", 0.8);
    let error = fixture.sync().unwrap_err();
    let mapped = sync_error(&error, Some("Hardcover: OK; Readwise Books"));
    assert_eq!(mapped.downcast_ref::<ExpectedError>().unwrap().as_ref(), code);
    assert!(mapped.to_string().contains("Hardcover: OK; Readwise Books:"));
    assert!(mapped.to_string().contains("POST /sessions"));
    if status == 401 {
      assert!(mapped.to_string().contains("Readwise token is invalid"));
    }
  }
}

#[test]
fn sync_preserves_real_times_fractions_and_privacy_and_never_replays_accepted_sessions() {
  for provider in ["readwise", "dual"] {
    let server = Server::new(vec![
      created_book(),
      created_session(),
      reading_book(),
      (500, "{}".into()),
      reading_book(),
      updated_book(),
      (200, json!({"results": [{"id": 4_790_033}]}).to_string()),
      created_session(),
      reading_book(),
      updated_book(),
    ]);
    let mut fixture = Fixture::new(&server);
    fixture.context.config.provider = provider.into();
    fixture.context.config.readwise_publish_to_feed = false;
    let path = fixture.queue("01", 0.447);
    fixture.context.config.readwise_publish_to_feed = Config::default().readwise_publish_to_feed;
    assert!(fixture.sync().is_err());
    assert!(path.with_extension("sent").exists());
    fixture.sync().unwrap();
    fixture.sync().unwrap();
    let path = fixture.queue("02", 1.0);
    let mut session: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    session["device_model"] = "Kobo Libra Colour".into();
    fs::write(&path, session.to_string()).unwrap();
    fixture.sync().unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 10);
    assert_eq!(
      requests[0],
      (
        "POST /books/api/v1/books HTTP/1.1".into(),
        json!({"items": [{
          "product": {"title": "Unmatched book", "author": "Test Author"},
          "status": "currently_reading"
        }]})
      )
    );
    assert_eq!(
      requests[1],
      (
        "POST /books/api/v1/sessions HTTP/1.1".into(),
        json!({"items": [{
          "book_id": 4790033, "started_at": "2026-09-10T20:14:05Z", "ended_at": "2026-09-10T20:58:35Z",
          "format": "ebook", "percent_at_start": 0.412, "percent_at_end": 0.447,
          "publish_to_feed": false, "device": {"type": "E-Reader", "vendor": "Kobo"}
        }]})
      )
    );
    assert_eq!(requests[3].0, "PATCH /books/api/v1/books HTTP/1.1");
    assert_eq!(
      requests[5].1,
      json!({"items": [{"id": 4790033, "status": "currently_reading"}]})
    );
    assert_eq!(
      requests[6],
      ("GET /books/api/v1/books?ids=4790033 HTTP/1.1".into(), Value::Null)
    );
    assert_eq!(requests[7].0, "POST /books/api/v1/sessions HTTP/1.1");
    assert_eq!(requests[7].1["items"][0]["book_id"], 4_790_033);
    assert_eq!(requests[7].1["items"][0]["publish_to_feed"], true);
    assert_eq!(
      requests[7].1["items"][0]["device"],
      json!({"type": "E-Reader", "vendor": "Kobo", "model": "Kobo Libra Colour"})
    );
    assert_eq!(
      requests[9].1,
      json!({"items": [{"id": 4790033, "status": "finished", "finished_reading_at": "2026-09-10T20:58:35Z"}]})
    );
    assert_eq!(
      fs::read_dir(fixture.client.directory.join("readwise-sessions"))
        .unwrap()
        .count(),
      0
    );
  }
}

#[test]
fn cached_book_must_be_verified_before_sending_and_failed_checks_keep_the_session_pending() {
  for response in [
    (200, json!({"results": []}).to_string()),
    (200, json!({"results": [{"id": 4_815_162}]}).to_string()),
    (500, "{}".into()),
  ] {
    let server = Server::new(vec![
      created_book(),
      created_session(),
      reading_book(),
      updated_book(),
      response,
      (200, json!({"results": [{"id": 4_790_033}]}).to_string()),
      created_session(),
      reading_book(),
      updated_book(),
    ]);
    let mut fixture = Fixture::new(&server);
    fixture.queue("01", 0.447);
    fixture.sync().unwrap();
    let path = fixture.queue("02", 0.8);
    assert!(fixture.sync().is_err());
    assert!(path.exists());
    assert!(!path.with_extension("sending").exists());
    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    assert_eq!(
      requests[4],
      ("GET /books/api/v1/books?ids=4790033 HTTP/1.1".into(), Value::Null)
    );

    fixture.sync().unwrap();
    assert!(!path.exists());
    let requests = server.requests();
    assert_eq!(requests.len(), 9);
    assert_eq!(requests[5].0, "GET /books/api/v1/books?ids=4790033 HTTP/1.1");
    assert_eq!(requests[6].0, "POST /books/api/v1/sessions HTTP/1.1");
    assert_eq!(requests[6].1["items"][0]["book_id"], 4_790_033);
    assert_eq!(requests[6].1["items"][0]["percent_at_end"], 0.8);
    assert_eq!(requests[8].0, "PATCH /books/api/v1/books HTTP/1.1");
  }
}

#[test]
fn ambiguous_session_outcomes_block_replay_and_newer_sessions() {
  for response in [
    (500, "{}".into()),
    // Even a body mentioning future times cannot prove rejection on HTTP 500.
    (
      500,
      json!({"error": {"code": "invalid_value", "message": "Session times must not be in the future"}}).to_string(),
    ),
    (0, String::new()),
    (200, "not-json".into()),
    (200, "{\"results\":[]}".into()),
    (200, "{\"results\":[{\"status\":\"created\"}]}".into()),
  ] {
    let server = Server::new(vec![created_book(), response]);
    let mut fixture = Fixture::new(&server);
    let path = fixture.queue("01", 0.447);
    assert!(fixture.sync().is_err());
    assert!(path.with_extension("sending").exists());
    fixture.queue("02", 0.8);
    assert!(fixture.sync().is_err());
    assert_eq!(server.requests().len(), 2);
  }
}

#[test]
fn batch_errors_are_not_success_and_rate_limits_persist_between_syncs() {
  // Documented Error object: the dialog and log must name the rejected request
  // and field, with remote text escaped for Nickel's HTML dialogs.
  let error = json!({
    "code": "invalid_value",
    "message": "Invalid value for items.0.book_id.",
    "details": [{"location": "body", "field": "items.0.book_id", "code": "invalid_value",
      "message": "Sessions cannot be recorded for a <b>sample</b>."}]
  });
  for (response, extension, expected) in [
    (
      (
        200,
        json!({"results": [{"status": "error", "error": error}]}).to_string(),
      ),
      "failed",
      "Readwise rejected POST /sessions: invalid_value: Invalid value for items.0.book_id. [body items.0.book_id: invalid_value — Sessions cannot be recorded for a &lt;b&gt;sample&lt;/b&gt;.]",
    ),
    (
      (400, json!({"error": error}).to_string()),
      "failed",
      "Readwise returned HTTP 400. Request: POST /sessions. Detail: invalid_value: Invalid value for items.0.book_id. [body items.0.book_id: invalid_value",
    ),
    ((429, "{}".into()), "json", "Request: POST /sessions."),
    ((401, "{}".into()), "json", "readwise_token"),
    ((403, "{}".into()), "json", "Readwise Books"),
  ] {
    let rate_limited = response.0 == 429;
    let server = Server::new(vec![created_book(), response]);
    let mut fixture = Fixture::new(&server);
    let path = fixture.queue("01", 0.447);
    let message = fixture.sync().unwrap_err().to_string();
    assert!(message.contains(expected), "{message}");
    assert!(path.with_extension(extension).exists());
    assert!(!path.with_extension("sent").exists());
    if rate_limited || extension == "failed" {
      assert!(fixture.sync().is_err());
      assert_eq!(server.requests().len(), 2);
    }
  }
}

#[test]
fn uncertain_unmatched_book_creation_is_not_replayed() {
  let server = Server::new(vec![(0, String::new())]);
  let mut fixture = Fixture::new(&server);
  fixture.queue("01", 0.447);
  assert!(fixture.sync().is_err());
  assert!(fixture.sync().unwrap_err().to_string().contains("uncertain"));
  assert_eq!(server.requests().len(), 1);
}

#[test]
fn invalid_session_and_concurrent_sync_do_not_write_remotely() {
  let server = Server::new(vec![]);
  let mut fixture = Fixture::new(&server);
  let path = fixture.queue("01", 44.7);
  assert!(
    fixture
      .sync()
      .unwrap_err()
      .to_string()
      .contains("its progress is outside 0 to 100%")
  );
  fs::remove_file(path.with_extension("failed")).unwrap();
  fixture.queue("01", 0.447);
  let lock = fs::File::options()
    .write(true)
    .open(fixture.client.directory.join("readwise-sync.lock"))
    .unwrap();
  lock.lock().unwrap();
  assert!(fixture.sync().unwrap_err().to_string().contains("already running"));
  assert!(server.requests().is_empty());
}

#[test]
fn config_keeps_hardcover_default_and_readwise_credentials_separate() {
  let config: Config = serini::from_str("access_token = hardcover-secret\n").unwrap();
  assert_eq!(config.provider, "hardcover");
  assert!(config.readwise_token.is_empty());
  assert!(config.readwise_publish_to_feed);
  let config: Config =
    serini::from_str("provider = readwise\nreadwise_token = test-token\nsync_annotations = true\n").unwrap();
  // Hardcover's journal setting must not send highlights to Readwise.
  assert!(!config.readwise_sync_annotations);
  assert!(config.readwise_publish_to_feed);
  let config: Config =
    serini::from_str("provider = readwise\nreadwise_token = test-token\nreadwise_publish_to_feed = false\n").unwrap();
  assert_eq!(config.provider, "readwise");
  assert!(config.access_token.is_empty());
  assert!(!config.readwise_publish_to_feed);
}

#[test]
fn highlights_are_sent_in_order_resent_after_a_failure_and_edited_notes_are_patched() {
  let created = (
    200,
    json!([{"id": 111, "title": "Moby Dick", "modified_highlights": [1337]}]).to_string(),
  );
  let listed = |whaling: Value, ishmael: Value| {
    (
      200,
      json!({"count": 3, "next": null, "results": [
        // Same second as a Kobo highlight but another text: must never be patched.
        {"id": 7, "text": "From another app", "note": "x", "highlighted_at": "2026-09-10T20:14:05Z"},
        {"id": 8, "text": "Whaling voyage", "note": whaling, "highlighted_at": "2026-09-10T20:14:05.379Z"},
        {"id": 9, "text": "Call me Ishmael.", "note": ishmael, "highlighted_at": "2026-09-11T08:00:00Z"}
      ]})
      .to_string(),
    )
  };
  let patched = (200, json!({"id": 8}).to_string());
  let server = Server::new(vec![
    (500, "{}".into()),
    created,
    listed("Old fate".into(), "Stale".into()),
    patched.clone(),
    patched,
    // Nothing new: Readwise may omit the book, so the cached ID is used.
    (200, "[]".into()),
    listed("Fate".into(), Value::Null),
  ]);
  let mut fixture = Fixture::new(&server);
  let book = "file:///mnt/onboard/moby.kepub.epub";
  let kobo = fixture.client.directory.join("KoboReader.sqlite");
  Connection::open(&kobo)
    .unwrap()
    .execute_batch(&format!(
      "CREATE TABLE content (ContentId TEXT, BookId TEXT, BookTitle TEXT, Title TEXT, Attribution TEXT,
        WordCount INTEGER, VolumeIndex INTEGER, ISBN TEXT);
      CREATE TABLE Bookmark (BookmarkID TEXT, VolumeID TEXT, ContentId TEXT, Text TEXT, Annotation TEXT,
        DateCreated TEXT, DateModified TEXT, ChapterProgress REAL, Hidden TEXT);
      INSERT INTO content VALUES ('{book}', NULL, NULL, 'Moby Dick', 'Herman Melville', -1, -1, NULL),
        ('ch1', '{book}', 'Moby Dick', 'Loomings', NULL, 1000, 1, NULL),
        ('ch2', '{book}', 'Moby Dick', 'The Carpet-Bag', NULL, 3000, 2, NULL);
      INSERT INTO Bookmark VALUES
        ('later', '{book}', 'ch2', '  Whaling voyage  ', 'Fate', '2026-09-10T20:14:05.000', '', 0.5, 'false'),
        ('first', '{book}', 'ch1', 'Call me Ishmael.', '', '2026-09-11T08:00:00.000', '', 0.2, 'false'),
        ('hidden', '{book}', 'ch1', 'Deleted on the Kobo', NULL, '2026-09-11T09:00:00.000', '', 0.3, 'true'),
        ('blank', '{book}', 'ch1', '   ', NULL, '2026-09-11T09:30:00.000', '', 0.4, 'false');"
    ))
    .unwrap();
  fixture.context.config.sqlite_path = kobo.to_string_lossy().into();

  let error = fixture.client.send_highlights(&mut fixture.context, book).unwrap_err();
  assert!(error.to_string().contains("next sync sends them again"));
  fixture.client.send_highlights(&mut fixture.context, book).unwrap();
  fixture.client.send_highlights(&mut fixture.context, book).unwrap();

  let highlight = |text: &str, highlighted_at: &str, location: i64| {
    json!({
      "text": text, "title": "Moby Dick", "author": "Herman Melville", "category": "books",
      "source_type": "bookwise_nickel", "highlighted_at": highlighted_at,
      "location": location, "location_type": "order"
    })
  };
  let mut later = highlight("Whaling voyage", "2026-09-10T20:14:05Z", 625_000);
  later["note"] = "Fate".into();
  let post = (
    "POST /api/v2/highlights/ HTTP/1.1".to_string(),
    json!({"highlights": [later, highlight("Call me Ishmael.", "2026-09-11T08:00:00Z", 50_000)]}),
  );
  let list = (
    "GET /api/v2/highlights/?book_id=111&page_size=1000&page=1 HTTP/1.1".to_string(),
    Value::Null,
  );
  assert_eq!(
    server.requests(),
    vec![
      post.clone(),
      post.clone(),
      list.clone(),
      ("PATCH /api/v2/highlights/8/ HTTP/1.1".into(), json!({"note": "Fate"})),
      ("PATCH /api/v2/highlights/9/ HTTP/1.1".into(), json!({"note": ""})),
      post,
      list,
    ]
  );
}

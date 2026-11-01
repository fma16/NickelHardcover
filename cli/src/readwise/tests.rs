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

#[test]
fn sync_preserves_real_times_fractions_and_privacy_and_never_replays_accepted_sessions() {
  let server = Server::new(vec![
    created_book(),
    created_session(),
    (500, "{}".into()),
    updated_book(),
    (200, json!({"results": [{"id": 4_790_033}]}).to_string()),
    created_session(),
    updated_book(),
  ]);
  let mut fixture = Fixture::new(&server);
  fixture.context.config.readwise_publish_to_feed = false;
  let path = fixture.queue("01", 0.447);
  fixture.context.config.readwise_publish_to_feed = Config::default().readwise_publish_to_feed;
  assert!(fixture.sync().is_err());
  assert!(path.with_extension("sent").exists());
  fixture.sync().unwrap();
  fixture.sync().unwrap();
  fixture.queue("02", 1.0);
  fixture.sync().unwrap();
  let requests = server.requests();
  assert_eq!(requests.len(), 7);
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
  assert_eq!(requests[2].0, "PATCH /books/api/v1/books HTTP/1.1");
  assert_eq!(
    requests[3].1,
    json!({"items": [{"id": 4790033, "status": "currently_reading"}]})
  );
  assert_eq!(
    requests[4],
    ("GET /books/api/v1/books?ids=4790033 HTTP/1.1".into(), Value::Null)
  );
  assert_eq!(requests[5].0, "POST /books/api/v1/sessions HTTP/1.1");
  assert_eq!(requests[5].1["items"][0]["book_id"], 4_790_033);
  assert_eq!(requests[5].1["items"][0]["publish_to_feed"], true);
  assert_eq!(
    requests[6].1,
    json!({"items": [{"id": 4790033, "status": "finished", "finished_reading_at": "2026-09-10T20:58:35Z"}]})
  );
  assert_eq!(
    fs::read_dir(fixture.client.directory.join("readwise-sessions"))
      .unwrap()
      .count(),
    0
  );
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
      updated_book(),
      response,
      (200, json!({"results": [{"id": 4_790_033}]}).to_string()),
      created_session(),
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
    assert_eq!(requests.len(), 4);
    assert_eq!(
      requests[3],
      ("GET /books/api/v1/books?ids=4790033 HTTP/1.1".into(), Value::Null)
    );

    fixture.sync().unwrap();
    assert!(!path.exists());
    let requests = server.requests();
    assert_eq!(requests.len(), 7);
    assert_eq!(requests[4].0, "GET /books/api/v1/books?ids=4790033 HTTP/1.1");
    assert_eq!(requests[5].0, "POST /books/api/v1/sessions HTTP/1.1");
    assert_eq!(requests[5].1["items"][0]["book_id"], 4_790_033);
    assert_eq!(requests[5].1["items"][0]["percent_at_end"], 0.8);
    assert_eq!(requests[6].0, "PATCH /books/api/v1/books HTTP/1.1");
  }
}

#[test]
fn ambiguous_session_outcomes_block_replay_and_newer_sessions() {
  for response in [
    (500, "{}".into()),
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
  for (response, extension) in [
    (
      (
        200,
        json!({"results": [{"status": "error", "error": {"code": "invalid_value"}}]}).to_string(),
      ),
      "failed",
    ),
    ((429, "{}".into()), "json"),
    ((401, "{}".into()), "json"),
    ((403, "{}".into()), "json"),
  ] {
    let rate_limited = response.0 == 429;
    let server = Server::new(vec![created_book(), response]);
    let mut fixture = Fixture::new(&server);
    let path = fixture.queue("01", 0.447);
    assert!(fixture.sync().is_err());
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
  assert!(fixture.sync().unwrap_err().to_string().contains("Invalid queued"));
  fs::remove_file(path).unwrap();
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
  let config: Config = serini::from_str("provider = readwise\nreadwise_token = test-token\n").unwrap();
  assert!(config.readwise_publish_to_feed);
  let config: Config =
    serini::from_str("provider = readwise\nreadwise_token = test-token\nreadwise_publish_to_feed = false\n").unwrap();
  assert_eq!(config.provider, "readwise");
  assert!(config.access_token.is_empty());
  assert!(!config.readwise_publish_to_feed);
}

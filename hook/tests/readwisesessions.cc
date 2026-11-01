#include "../src/readwisesessions.h"

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QJsonDocument>
#include <QTemporaryDir>
#include <QThread>
#include <cassert>
#include <iostream>

int main(int argc, char **argv) {
  QCoreApplication app(argc, argv);
  QTemporaryDir dir;
  assert(dir.isValid());
  ReadwiseSessions sessions(dir.path());
  sessions.begin("book-a", "Title", "Author", "4790033", false, 41);
  QThread::msleep(10);
  sessions.observe(44);
  assert(sessions.finish(true));
  assert(sessions.finish(true)); // Closing again must not duplicate the interval.
  QStringList files = QDir(dir.path()).entryList({"*.json"}, QDir::Files);
  assert(files.size() == 1);
  QFile file(QDir(dir.path()).filePath(files.first()));
  assert(file.open(QIODevice::ReadOnly));
  QJsonObject data = QJsonDocument::fromJson(file.readAll()).object();
  assert(data["content_id"] == "book-a");
  assert(data["linked_id"].toDouble() == 4790033);
  assert(data["percent_at_start"].toDouble() == 0.41);
  assert(data["percent_at_end"].toDouble() == 0.44);
  assert(!data["publish_to_feed"].toBool());
  assert(data["started_at"].toString() < data["ended_at"].toString());
  QString firstEnd = data["ended_at"].toString();

  QThread::msleep(30); // Simulates leaving ReadingView/sleeping; not reading time.
  sessions.begin("book-b", "Other", "Writer", "", true, 0);
  sessions.observe(1);
  QThread::msleep(10);
  assert(sessions.finish(true));
  files = QDir(dir.path()).entryList({"*.json"}, QDir::Files, QDir::Name);
  assert(files.size() == 2);
  QFile second(QDir(dir.path()).filePath(files.last()));
  assert(second.open(QIODevice::ReadOnly));
  data = QJsonDocument::fromJson(second.readAll()).object();
  assert(data["content_id"] == "book-b");
  assert(data["percent_at_start"].toDouble() == 0.0);
  assert(data["percent_at_end"].toDouble() == 0.01);
  assert(data["publish_to_feed"].toBool());
  assert(!data.contains("linked_id"));
  assert(data["started_at"].toString() > firstEnd);

  for (int end : {44, 40}) {
    sessions.begin("book-c", "Title", "Author", "", false, 44);
    sessions.observe(end);
    QThread::msleep(2);
    assert(sessions.finish(true));
  }
  sessions.begin("book-c", "Title", "Author", "", false, 44);
  sessions.observe(50);
  assert(sessions.finish(false)); // Auto-sync disabled: do not persist.
  assert(QDir(dir.path()).entryList({"*.json"}, QDir::Files).size() == 2);

  QFile blocked(dir.filePath("not-a-directory"));
  assert(blocked.open(QIODevice::WriteOnly));
  blocked.close();
  ReadwiseSessions failure(blocked.fileName());
  failure.begin("book-d", "Title", "Author", "", false, 98);
  failure.observe(99);
  QThread::msleep(2);
  assert(!failure.finish(true));
  assert(failure.active()); // A storage failure must not silently discard reading.
  std::cout << "Readwise capture: fractions, sleep boundaries, book changes, consent, duplicate close and storage "
               "failure OK\n";
}

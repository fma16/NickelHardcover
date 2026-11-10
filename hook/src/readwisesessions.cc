#include "readwisesessions.h"

#include <QDir>
#include <QJsonDocument>
#include <QSaveFile>
#include <QUuid>

void ReadwiseSessions::begin(const QString &contentId, const QString &title, const QString &author,
                             const QString &linkedId, const QString &deviceModel, bool publish, int progress) {
  if (active())
    return;
  started = QDateTime::currentDateTimeUtc();
  startProgress = endProgress = qBound(0, progress, 100);
  data = QJsonObject();
  data.insert("content_id", contentId);
  data.insert("title", title);
  data.insert("author", author);
  if (!deviceModel.isEmpty())
    data.insert("device_model", deviceModel);
  data.insert("publish_to_feed", publish);
  data.insert("started_at", started.toString("yyyy-MM-dd'T'HH:mm:ss.zzz'Z'"));
  data.insert("percent_at_start", startProgress / 100.0);
  bool ok = false;
  qlonglong id = linkedId.toLongLong(&ok);
  if (ok && id > 0)
    data.insert("linked_id", static_cast<double>(id));
}

bool ReadwiseSessions::finish(bool save) {
  if (!active())
    return true;
  QDateTime ended = QDateTime::currentDateTimeUtc();
  if (save && ended > started && endProgress > startProgress) {
    // Freeze a completed interval even if storage fails: later retries must not
    // count time spent asleep or in another book.
    if (!data.contains("ended_at")) {
      data.insert("ended_at", ended.toString("yyyy-MM-dd'T'HH:mm:ss.zzz'Z'"));
      data.insert("percent_at_end", endProgress / 100.0);
    }
    if (!QDir().mkpath(directory))
      return false;
    QString name = started.toString("yyyyMMddHHmmsszzz") + "-" + QUuid::createUuid().toString() + ".json";
    QSaveFile file(QDir(directory).filePath(name));
    QByteArray bytes = QJsonDocument(data).toJson(QJsonDocument::Compact);
    if (!file.open(QIODevice::WriteOnly) || file.write(bytes) != bytes.size() || !file.commit())
      return false;
  }
  started = QDateTime();
  return true;
}

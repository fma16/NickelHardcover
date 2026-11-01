#pragma once

#include <QDateTime>
#include <QJsonObject>
#include <QString>

// A completed interval in ReadingView, not the time between Wi-Fi syncs.
// This class uses only public Qt APIs; Nickel integration lives in SyncController.
class ReadwiseSessions {
public:
  explicit ReadwiseSessions(const QString &directory) : directory(directory) {}
  void begin(const QString &contentId, const QString &title, const QString &author, const QString &linkedId,
             bool publish, int progress);
  void observe(int progress) {
    if (!data.contains("ended_at"))
      endProgress = qBound(0, progress, 100);
  }
  bool finish(bool save);
  bool active() const { return started.isValid(); }
  QString contentId() const { return data.value("content_id").toString(); }

private:
  QString directory;
  QJsonObject data;
  QDateTime started;
  int startProgress = 0;
  int endProgress = 0;
};

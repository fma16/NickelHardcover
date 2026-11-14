#pragma once

#include <QLabel>
#include <QNetworkAccessManager>
#include <QObject>
#include <QSettings>

#include "files.h"
#include "nickelhardcover.h"
#include "readwisesessions.h"
#include "syncqueue.h"

struct BookInfo {
  QString title;
  QString author;
};

class SyncController : public QObject {
  Q_OBJECT

public:
  static SyncController *getInstance();

  QString contentId;
  bool notRealBook;
  QNetworkAccessManager *network = new QNetworkAccessManager(this);

  void registerBook(const QString &contentId, const QString &title, const QString &author) {
    books[contentId] = {title, author};
  }

  BookInfo getBookInfo(const QString &contentId) const { return books.value(contentId); }

  int getCurrentProgress() const { return primaryQueue()->getProgress().value(contentId); }

  const QHash<QString, int> &getProgress(bool readwise) const {
    return (readwise ? readwiseQueue : hardcoverQueue)->getProgress();
  }

  void clearReadProgress(const QString &contentId) { primaryQueue()->clearReadProgress(contentId); }
  void clearReadwiseProgress(const QString &contentId) { readwiseQueue->clearReadProgress(contentId); }

  void manualSync();
  void finishReadwiseSession(bool manual = false);
  QDateTime getAlarm() const;

  bool isEnabled() const {
    return !notRealBook && ((hardcoverQueue->available() && !hardcoverQueue->running) ||
                            (readwiseQueue->available() && !readwiseQueue->running));
  }

  void currentViewIndexChanged(int index);

public Q_SLOTS:
  void pageChanged();
  void alarm();

Q_SIGNALS:
  void currentViewChanged(const QString &name);

private:
  SyncController(QObject *parent = nullptr);

  PowerTimer *timer = nullptr;
  SyncQueue *hardcoverQueue = new SyncQueue(false, this);
  SyncQueue *readwiseQueue = new SyncQueue(true, this);
  SyncQueue *primaryQueue() const;

  QHash<QString, BookInfo> books;

  ReadwiseSessions readwise{QString(Files::adds_directory) + "/readwise-sessions"};
  void beginReadwiseSession(QWidget *view);
  QString lastViewName;
  int lastSyncDaily = 0;
};

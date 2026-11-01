#include <NickelHook.h>
#include <QDateTime>
#include <QLabel>
#include <QSettings>
#include <QTimer>

#include "settings.h"
#include "synccontroller.h"

SyncController *SyncController::getInstance() {
  static SyncController instance;
  return &instance;
}

SyncController::SyncController(QObject *parent) : QObject(parent) {};

void SyncController::currentViewIndexChanged(int index) {
  if (index < 0)
    return;

  queue->failed = false;

  MainWindowController *mwc = MainWindowController__sharedInstance();
  QWidget *cv = MainWindowController__currentView(mwc);
  QString name = cv->objectName();
  nh_log("Current view changed to %s last view was %s", qPrintable(name), qPrintable(lastViewName));

  if (name != "ReadingView") {
    finishReadwiseSession();
  }

  if (name != lastViewName) {
    currentViewChanged(name);
  }

  QDateTime alarm = getAlarm();
  if (name.endsWith("DragonPowerView") && alarm.isValid()) {
    nh_log("Alarm set for %s", qPrintable(alarm.toString()));
  }

  if (notRealBook) {
    return;
  }

  Settings *settings = Settings::getInstance();
  if (lastViewName == "ReadingView" &&
      (name.endsWith("DragonPowerView") || MainWindowController__viewWithObjectName(mwc, "ReadingView") == nullptr) &&
      settings->isEnabled(contentId) && queue->checkThreshold(contentId, settings->getSyncOnClose())) {
    nh_log("Triggered on close auto-sync");
    queue->run(contentId);
  }

  if (name == "ReadingView") {
    beginReadwiseSession(cv);
    if (settings->isEnabled(contentId)) {
      queue->updateReadProgress(contentId);
    }

    QObject::connect(cv, SIGNAL(pageChanged(int)), this, SLOT(pageChanged()), Qt::UniqueConnection);
  }

  lastViewName = name;
}

void SyncController::pageChanged() {
  nh_log("SyncController::pageChanged()");

  if (notRealBook) {
    return;
  }

  Settings *settings = Settings::getInstance();

  if (settings->isReadwise()) {
    MainWindowController *mwc = MainWindowController__sharedInstance();
    QWidget *view = MainWindowController__currentView(mwc);
    if (view->objectName() == "ReadingView") {
      beginReadwiseSession(view);
      readwise.observe(ReadingView__getCalculatedReadProgress(view));
    }
  }

  if (!settings->isEnabled(contentId))
    return;

  MainWindowController *mwc = MainWindowController__sharedInstance();
  QWidget *cv = MainWindowController__currentView(mwc);
  QString name = cv->objectName();
  if (name == "ReadingView") {
    queue->updateReadProgress(contentId);
  }

  int syncDaily = settings->getSyncOnSchedule();

  if (timer != nullptr && (PowerTimer__timeRemaining(timer) <= 0 || syncDaily != lastSyncDaily)) {
    timer->deleteLater();
    timer = nullptr;
  }

  lastSyncDaily = syncDaily;

  if (timer == nullptr && syncDaily >= 0 && queue->checkThreshold(contentId, 1)) {
    timer = construct_PowerTimer("NickelHardcover-alarm", this);

    QDateTime time = QDateTime::currentDateTime();
    if (time.time().hour() >= syncDaily) {
      time = time.addDays(1);
    }
    time.setTime(QTime(syncDaily, 0));
    nh_log("Setting alarm for %s", qPrintable(time.toString()));

    PowerTimer__fireAt(timer, time);
    QObject::connect(timer, SIGNAL(timeout()), this, SLOT(alarm()));
  }

  if (!queue->failed && ((getCurrentProgress() == 100 && settings->getLastProgress(contentId) != 100) ||
                         queue->checkThreshold(contentId, settings->getSyncOnRead()))) {
    nh_log("Triggered threshold auto-sync");
    finishReadwiseSession();
    beginReadwiseSession(cv);
    queue->run(contentId);
  }
}

void SyncController::alarm() {
  nh_log("SyncController::alarm()");

  timer->deleteLater();
  timer = nullptr;
  finishReadwiseSession();
  beginReadwiseSession(MainWindowController__currentView(MainWindowController__sharedInstance()));
  queue->runAll();
}

void SyncController::manualSync() {
  nh_log("SyncController::manualSync()");
  finishReadwiseSession(true);
  MainWindowController *mwc = MainWindowController__sharedInstance();
  beginReadwiseSession(MainWindowController__currentView(mwc));
  queue->updateReadProgress(contentId);
  queue->run(contentId, true);
}

void SyncController::beginReadwiseSession(QWidget *view) {
  Settings *settings = Settings::getInstance();
  if (!settings->isReadwise() || !settings->isSignedIn() || notRealBook || view->objectName() != "ReadingView")
    return;
  BookInfo book = getBookInfo(contentId);
  readwise.begin(contentId, book.title, book.author, settings->getLinkedId(contentId),
                 settings->publishReadwiseSessions(), ReadingView__getCalculatedReadProgress(view));
}

void SyncController::finishReadwiseSession(bool manual) {
  Settings *settings = Settings::getInstance();
  if (!readwise.finish(manual || (settings->isReadwise() && settings->isEnabled(readwise.contentId())))) {
    showBookErrorDialog(readwise.contentId(), "Cannot save the Readwise session. Check free storage before closing the book.");
  }
}

QDateTime SyncController::getAlarm() const {
  if (timer != nullptr && PowerTimer__timeRemaining(timer) > 0) {
    return QDateTime::currentDateTime().addMSecs(PowerTimer__timeRemaining(timer));
  }

  return QDateTime();
}

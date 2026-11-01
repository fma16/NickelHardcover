#include <QSettings>
#include <QTimer>
#include <QDir>

#include <NickelHook.h>

#include "settings.h"
#include "syncqueue.h"

SyncQueue::SyncQueue(QObject *parent) : QObject(parent) {
  WirelessManager *wm = WirelessManager__sharedInstance();
  QObject::connect(wm, SIGNAL(networkConnected()), this, SLOT(networkConnected()));
}

void SyncQueue::updateReadProgress(const QString &contentId) {
  MainWindowController *mwc = MainWindowController__sharedInstance();
  QWidget *cv = MainWindowController__currentView(mwc);

  int newProgress = ReadingView__getCalculatedReadProgress(cv);
  if (newProgress == 99 && !Settings::getInstance()->isReadwise()) {
    newProgress = 100;
  }

  if ((!Settings::getInstance()->isReadwise() && newProgress < 2) || newProgress == progress.value(contentId)) {
    return;
  }

  progress[contentId] = newProgress;

  nh_log("Update %s queued progress to %d%%", qPrintable(contentId), progress.value(contentId));
}

bool SyncQueue::checkThreshold(const QString &contentId, int threshold) const {
  return threshold > 0 && progress.value(contentId) > 0 &&
         abs(Settings::getInstance()->getLastProgress(contentId) - progress.value(contentId)) >= threshold;
}

void SyncQueue::networkConnected() {
  if (Settings::getInstance()->isReadwise() && Settings::getInstance()->isSignedIn() &&
      Settings::getInstance()->getRetryOnNetwork()) {
    QDir spool(QString(Files::adds_directory) + "/readwise-sessions");
    if (!spool.entryList({"*.json", "*.sent"}, QDir::Files).isEmpty())
      run(QString());
    return;
  }
  if (retryQueue.isEmpty() || !Settings::getInstance()->getRetryOnNetwork())
    return;

  nh_log("Retrying %d items", retryQueue.size());

  foreach (const QString &value, retryQueue) {
    queue.insert(value);
  }

  retryQueue.clear();
  prepareNext();
}

void SyncQueue::runAll() {
  if (Settings::getInstance()->isReadwise()) {
    run(QString());
    return;
  }
  for (QHash<QString, int>::const_iterator it = progress.constBegin(); it != progress.constEnd(); ++it) {
    queue.insert(it.key());
  }

  prepareNext();
}

void SyncQueue::prepareNext() {
  QObject::connect(this, &SyncQueue::finished, this, &SyncQueue::prepareNext, Qt::UniqueConnection);

  if (queue.isEmpty()) {
    nh_log("No more items in queue");
    QObject::disconnect(this, &SyncQueue::finished, this, &SyncQueue::prepareNext);
  } else {
    QString contentId = *queue.begin();
    queue.remove(contentId);

    nh_log("Syncing %s", qPrintable(contentId));
    run(contentId);
  }
}

void SyncQueue::run(const QString &contentId, bool manual) {
  if (running) {
    return;
  }

  retryQueue.remove(contentId);
  currentProgress = progress.value(contentId);

  if (currentProgress == 0 && !Settings::getInstance()->isReadwise()) {
    nh_log("Attempted to sync %s with no saved reading progress", qPrintable(contentId));
    showBookErrorDialog(contentId, "Reading progress must be at least 2% to sync with Hardcover.app");
    finished();
    return;
  }

  failed = false;
  running = true;
  this->currentContentId = contentId;

  if (manual) {
    dialog = ConfirmationDialogFactory__getConfirmationDialog(nullptr);
    ConfirmationDialog__showCloseButton(dialog, false);
    ConfirmationDialog__setText(dialog, "Syncing with " + Settings::getInstance()->serviceName() + "...");
    dialog->open();
  }

  CLI *cli = CLI::update(contentId, currentProgress, !manual);
  QObject::connect(cli, &CLI::success, this, &SyncQueue::success);
  QObject::connect(cli, &CLI::failure, this, &SyncQueue::failure);
}

void SyncQueue::success() {
  running = false;

  if (currentProgress == 100 && !Settings::getInstance()->isReadwise()) {
    Settings::getInstance()->setEnabled(currentContentId, false);
  }

  Settings::getInstance()->setLastProgress(currentContentId, currentProgress);
  if (!Settings::getInstance()->isReadwise() || progress.value(currentContentId) == currentProgress)
    progress.remove(currentContentId);

  finished();

  if (dialog == nullptr)
    return;

  ConfirmationDialog__setText(dialog, "Success!");
  QTimer::singleShot(800, this, &SyncQueue::closeDialog);
}

void SyncQueue::failure(CLI::FailureReason reason) {
  failed = true;
  running = false;

  if (reason == CLI::FailureReason::Network && Settings::getInstance()->getRetryOnNetwork()) {
    retryQueue.insert(currentContentId);
  } else {
    progress.remove(currentContentId);
  }

  closeDialog();
  finished();
}

void SyncQueue::closeDialog() {
  if (dialog == nullptr)
    return;

  dialog->close();
  dialog->deleteLater();
  dialog = nullptr;
}

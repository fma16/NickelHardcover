#include <QDir>
#include <QSettings>
#include <QTimer>

#include <NickelHook.h>

#include "settings.h"
#include "syncqueue.h"

SyncQueue::SyncQueue(bool readwise, QObject *parent) : QObject(parent), readwise(readwise) {
  WirelessManager *wm = WirelessManager__sharedInstance();
  QObject::connect(wm, SIGNAL(networkConnected()), this, SLOT(networkConnected()));
}

bool SyncQueue::available() const {
  Settings *settings = Settings::getInstance();
  return settings->usesService(readwise) && settings->isSignedIn(readwise);
}

bool SyncQueue::enabled(const QString &contentId) const {
  return available() && Settings::getInstance()->isEnabled(contentId, readwise);
}

void SyncQueue::updateReadProgress(const QString &contentId) {
  MainWindowController *mwc = MainWindowController__sharedInstance();
  QWidget *cv = MainWindowController__currentView(mwc);

  int newProgress = ReadingView__getCalculatedReadProgress(cv);
  if (newProgress == 99 && !readwise) {
    newProgress = 100;
  }

  if ((!readwise && newProgress < 2) || newProgress == progress.value(contentId)) {
    return;
  }

  progress[contentId] = newProgress;

  nh_log("Update %s queued progress to %d%%", qPrintable(contentId), progress.value(contentId));
}

bool SyncQueue::checkThreshold(const QString &contentId, int threshold) const {
  return threshold > 0 && progress.value(contentId) > 0 &&
         abs(Settings::getInstance()->getLastProgress(contentId, readwise) - progress.value(contentId)) >= threshold;
}

void SyncQueue::networkConnected() {
  if (!available() || !Settings::getInstance()->getRetryOnNetwork())
    return;
  if (readwise) {
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
  if (readwise) {
    run(QString());
    return;
  }
  for (QHash<QString, int>::const_iterator it = progress.constBegin(); it != progress.constEnd(); ++it) {
    queue.insert(it.key());
  }

  prepareNext();
}

void SyncQueue::prepareNext() {
  if (running)
    return;
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
  if (!available())
    return;
  if (running) {
    // A deferred Readwise drain must not acknowledge progress from an interval
    // that may still be active when the previous request completes.
    queue.insert(readwise ? QString() : contentId);
    QObject::connect(this, &SyncQueue::finished, this, &SyncQueue::prepareNext, Qt::UniqueConnection);
    return;
  }

  retryQueue.remove(contentId);
  currentProgress = progress.value(contentId);

  if (currentProgress == 0 && !readwise) {
    nh_log("Attempted to sync %s with no saved reading progress", qPrintable(contentId));
    if (manual)
      showBookErrorDialog(contentId, "Reading progress must be at least 2% to sync with Hardcover.app");
    finished();
    return;
  }

  failed = false;
  running = true;
  this->currentContentId = contentId;

  if (manual) {
    closeDialog();
    dialog = ConfirmationDialogFactory__getConfirmationDialog(nullptr);
    ConfirmationDialog__showCloseButton(dialog, false);
    ConfirmationDialog__setText(dialog, readwise ? "Syncing with Readwise Books..." : "Syncing with Hardcover.app...");
    dialog->open();
  }

  CLI *cli = CLI::update(contentId, currentProgress, !manual, readwise);
  QObject::connect(cli, &CLI::success, this, &SyncQueue::success);
  QObject::connect(cli, &CLI::failure, this, &SyncQueue::failure);
}

void SyncQueue::success() {
  running = false;

  if (currentProgress == 100 && !readwise) {
    Settings::getInstance()->setEnabled(currentContentId, false, false);
  }

  if (!currentContentId.isEmpty()) {
    Settings::getInstance()->setLastProgress(currentContentId, currentProgress, readwise);
    if (progress.value(currentContentId) == currentProgress)
      progress.remove(currentContentId);
  }

  if (dialog != nullptr) {
    ConfirmationDialog__setText(dialog, readwise ? "Readwise Books: success!" : "Hardcover.app: success!");
    QTimer::singleShot(800, this, &SyncQueue::closeDialog);
  }
  finished();
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

// Exercise the real controller, queues, settings and session capture against
// simulated Nickel views and independently completed CLI requests. No account
// credentials, Kobo paths or remote APIs are used by this native test.
#include "src/cli.h"
#include "src/settings.h"
#include "src/synccontroller.h"
#include <QApplication>
#include <QDir>
#include <QEventLoop>
#include <QFile>
#include <QJsonDocument>
#include <QPointer>
#include <QThread>
#include <QTimer>
#include <cassert>
#include <iostream>

class Wireless : public QObject {
  Q_OBJECT
Q_SIGNALS:
  void networkConnected();
};
class ReadingView : public QWidget {
  Q_OBJECT
Q_SIGNALS:
  void pageChanged(int);
};
static Wireless wireless;
static QWidget *view;
static int percentage;
static CLI *pending[2] = {};
static bool pendingSilent[2] = {};

extern "C" void nh_log(const char *, ...) {}
MainWindowController *(*MainWindowController__sharedInstance)() = []() -> QObject * { return &wireless; };
QWidget *(*MainWindowController__currentView)(MainWindowController *) = [](QObject *) { return view; };
QWidget *(*MainWindowController__viewWithObjectName)(MainWindowController *, const QString &) =
    [](QObject *, const QString &) -> QWidget * { return nullptr; };
WirelessManager *(*WirelessManager__sharedInstance)() = []() -> QObject * { return &wireless; };
int (*ReadingView__getCalculatedReadProgress)(QWidget *) = [](QWidget *) { return percentage; };
QString getDeviceModel() { return "Kobo Libra Colour"; }
PowerTimer *construct_PowerTimer(const QString &, QObject *parent) { return new QObject(parent); }
int (*PowerTimer__timeRemaining)(PowerTimer *) = [](QObject *) { return 1000; };
void (*PowerTimer__fireAt)(PowerTimer *, const QDateTime &) = [](QObject *, const QDateTime &) {};
ConfirmationDialog *(*ConfirmationDialogFactory__getConfirmationDialog)(QWidget *) = [](QWidget *parent) {
  return new QDialog(parent);
};
void (*ConfirmationDialog__showCloseButton)(ConfirmationDialog *, bool) = [](QDialog *, bool) {};
void (*ConfirmationDialog__setText)(ConfirmationDialog *, const QString &) = [](QDialog *, const QString &) {};
void showBookErrorDialog(const QString &, const QString &) { assert(false && "Unexpected error dialog"); }

CLI::CLI(QStringList, bool, bool, const QString &, QObject *parent) : QObject(parent) {}
CLI::~CLI() = default;
void CLI::networkConnected() {}
void CLI::connectingFailed() {}
CLI *CLI::update(const QString &, int, bool silent, bool readwise) {
  assert(!pending[readwise]);
  pendingSilent[readwise] = silent;
  return pending[readwise] = new CLI({});
}
static void complete(bool readwise, bool success, CLI::FailureReason reason = CLI::Error) {
  CLI *cli = pending[readwise];
  assert(cli);
  pending[readwise] = nullptr;
  if (success)
    cli->success();
  else
    cli->failure(reason);
  cli->deleteLater();
}

int main(int argc, char **argv) {
  QApplication app(argc, argv);
  ReadingView readingView;
  view = &readingView;
  QSettings config(Files::config, QSettings::IniFormat);
  config.setValue("provider", "dual");
  config.setValue("access_token", "fake-hardcover");
  config.setValue("readwise_token", "fake-readwise");
  config.setValue("auto_sync_default", true);
  config.setValue("retry_on_network", true);
  config.setValue("sync_on_close", 1);
  config.setValue("sync_on_read", -1);
  config.sync();
  QSettings library(Files::library, QSettings::IniFormat);
  library.setValue("book/linkedbook", "123");
  library.setValue("readwise/book/linkedbook", "4790033");
  library.setValue("book/progress", 10);
  library.setValue("readwise/book/progress", 45);
  library.sync();

  auto ctl = SyncController::getInstance();
  auto settings = Settings::getInstance();
  ctl->notRealBook = false;
  for (const QString &scenario : {QString("first-page"), QString("deferred-manual"), QString("anonymous-drain")}) {
    SyncQueue queue(true);
    if (scenario == "first-page") {
      for (bool readwise : {false, true}) {
        SyncQueue first(readwise);
        percentage = 48;
        first.updateReadProgress("fresh");
        percentage = 49;
        first.updateReadProgress("fresh");
        assert(!first.checkThreshold("fresh", 20));
        percentage = 67;
        first.updateReadProgress("fresh");
        assert(!first.checkThreshold("fresh", 20));
        percentage = 68;
        first.updateReadProgress("fresh");
        assert(first.checkThreshold("fresh", 20));
        settings->setLastProgress("fresh", 0, readwise);
        assert(first.checkThreshold("fresh", 50)); // A real saved zero is not an absent baseline.
      }
    } else if (scenario == "deferred-manual") {
      percentage = 42;
      queue.updateReadProgress("busy");
      queue.run("busy");
      assert(pendingSilent[true]);
      queue.run("busy", true);
      assert(QApplication::activeModalWidget());
      complete(true, true);
      assert(!pendingSilent[true]);
      QPointer<QWidget> deferredDialog = QApplication::activeModalWidget();
      QEventLoop loop;
      QTimer::singleShot(900, &loop, SLOT(quit()));
      loop.exec();
      // The previous request's 800ms success timer must not close this dialog.
      assert(deferredDialog && deferredDialog->isVisible());
      complete(true, true);
    } else if (scenario == "anonymous-drain") {
      percentage = 43;
      queue.updateReadProgress("offline");
      queue.run("offline");
      complete(true, false, CLI::Network);
      queue.runAll();
      complete(true, true);
      // A successful anonymous drain cannot acknowledge an active interval.
      assert(settings->getLastProgress("offline", true) == 0);
      assert(queue.getProgress().value("offline") == 43);
    }
    std::cout << qPrintable(scenario) << " OK\n";
  }
  ctl->contentId = "book";
  ctl->registerBook("book", "Title", "Author");
  assert(settings->getLinkedId("book") == "123");
  assert(settings->getLinkedId("book", true) == "4790033");

  auto enter = [&](int progress) {
    percentage = progress;
    view->setObjectName("ReadingView");
    ctl->currentViewIndexChanged(0);
    QThread::msleep(2);
  };
  auto close = [&] {
    view->setObjectName("DragonPowerView");
    ctl->currentViewIndexChanged(1);
  };

  enter(50);
  percentage = 54;
  readingView.pageChanged(54);
  close();
  assert(!ctl->isEnabled());
  complete(false, false);
  assert(ctl->isEnabled()); // The available service remains usable while the other runs.
  assert(ctl->getProgress(false).isEmpty());
  assert(ctl->getProgress(true).value("book") == 54);
  ctl->clearReadProgress("book");
  assert(ctl->getProgress(true).value("book") == 54); // Clearing Hardcover never discards Readwise.
  complete(true, true);
  assert(settings->getLastProgress("book", false) == 10);
  assert(settings->getLastProgress("book", true) == 54);
  QDir spool(QString(Files::adds_directory) + "/readwise-sessions");
  auto files = spool.entryList({"*.json"}, QDir::Files);
  assert(files.size() == 1);
  QFile file(spool.filePath(files.first()));
  assert(file.open(QIODevice::ReadOnly));
  auto session = QJsonDocument::fromJson(file.readAll()).object();
  assert(session["linked_id"].toDouble() == 4790033);
  assert(session["percent_at_start"].toDouble() == 0.50);
  assert(session["percent_at_end"].toDouble() == 0.54);
  assert(session["device_model"] == "Kobo Libra Colour");
  assert(session["publish_to_feed"].toBool());

  enter(98);
  percentage = 99;
  readingView.pageChanged(99);
  complete(false, true); // Hardcover still rounds 99 to 100 and disables itself.
  close();
  complete(true, false); // Readwise failure cannot undo Hardcover's success.
  assert(settings->getLastProgress("book", false) == 100);
  assert(settings->getLastProgress("book", true) == 54);
  assert(!settings->isEnabled("book", false));
  assert(settings->isEnabled("book", true));

  // Below 2%, Hardcover retains its upstream minimum; Readwise captures 0→1%.
  ctl->contentId = "new-book";
  enter(0);
  percentage = 1;
  readingView.pageChanged(1);
  close();
  complete(true, true);
  assert(settings->getLastProgress("new-book", false) == 0);
  assert(settings->getLastProgress("new-book", true) == 1);

  // An offline Hardcover retry and a pending Readwise drain complete separately.
  ctl->contentId = "offline";
  enter(40);
  percentage = 43;
  readingView.pageChanged(43);
  close();
  complete(false, false, CLI::Network);
  complete(true, false, CLI::Network);
  wireless.networkConnected();
  complete(true, true);
  complete(false, true);
  assert(settings->getLastProgress("offline", false) == 43);
  assert(!pending[false] && !pending[true]);

  // Manual sync works even when per-book automatic sync is disabled.
  enter(46);
  settings->setEnabled("offline", false, false);
  settings->setEnabled("offline", false, true);
  ctl->manualSync();
  complete(true, true);
  complete(false, true);
  assert(settings->getLastProgress("offline", false) == 46);
  assert(settings->getLastProgress("offline", true) == 46);

  // A request queued during an in-flight send must not erase newer progress.
  SyncQueue hardcover(false), readwise(true);
  percentage = 60;
  hardcover.updateReadProgress("overlap");
  readwise.updateReadProgress("overlap");
  hardcover.run("overlap");
  readwise.run("overlap");
  percentage = 63;
  hardcover.updateReadProgress("overlap");
  readwise.updateReadProgress("overlap");
  hardcover.run("overlap");
  readwise.run("overlap");
  complete(false, true);
  complete(true, true);
  complete(false, true);
  complete(true, true);
  assert(settings->getLastProgress("overlap", false) == 63);
  assert(settings->getLastProgress("overlap", true) == 60);
  assert(readwise.getProgress().value("overlap") == 63);
  close();

  // Disabling a provider preserves its library and credentials. Signing out
  // of either account must leave the other available, including in dual mode.
  for (const QString &provider : {QString("hardcover"), QString("readwise"), QString("dual")}) {
    config.setValue("provider", provider);
    config.sync();
    ctl->currentViewChanged("ReadingView");
    assert(hardcover.available() == (provider != "readwise"));
    assert(readwise.available() == (provider != "hardcover"));
    assert(settings->getLinkedId("book", false) == "123");
    assert(settings->getLinkedId("book", true) == "4790033");
    ctl->contentId = provider + "-only";
    enter(30);
    percentage = 34;
    readingView.pageChanged(34);
    close();
    if (hardcover.available())
      complete(false, true);
    if (readwise.available())
      complete(true, true);
    assert(settings->getLastProgress(ctl->contentId, false) == (provider == "readwise" ? 0 : 34));
    assert(settings->getLastProgress(ctl->contentId, true) == (provider == "hardcover" ? 0 : 34));
  }
  settings->clearAccessToken(false);
  assert(!hardcover.available() && readwise.available());
  config.setValue("access_token", "fake-hardcover");
  config.sync();
  ctl->currentViewChanged("ReadingView");
  settings->clearAccessToken(true);
  assert(hardcover.available() && !readwise.available());
  std::cout
      << "Dual sync: independent failures/retries, progress, links, tokens, capture and single-service modes OK\n";
}

#include "dualsync.moc"

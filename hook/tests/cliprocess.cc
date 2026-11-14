#include "src/cli.h"
#include "src/settings.h"
#include "src/synccontroller.h"
#include <QApplication>
#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QThread>
#include <cassert>
#include <iostream>

class Wireless : public QObject {
  Q_OBJECT
Q_SIGNALS:
  void networkConnected();
};
static Wireless wireless;
static QWidget *view;
static QString errorTitle;
static int dialogs;
extern "C" void nh_log(const char *, ...) {}
MainWindowController *(*MainWindowController__sharedInstance)() = []() -> QObject * { return &wireless; };
QWidget *(*MainWindowController__currentView)(MainWindowController *) = [](QObject *) { return view; };
QWidget *(*MainWindowController__viewWithObjectName)(MainWindowController *, const QString &) =
    [](QObject *, const QString &) -> QWidget * { return nullptr; };
WirelessManager *(*WirelessManager__sharedInstance)() = []() -> QObject * { return &wireless; };
WirelessWorkflowManager *(*WirelessWorkflowManager__sharedInstance)() = []() -> QObject * { return &wireless; };
bool (*WirelessWorkflowManager__isInternetAccessible)(WirelessWorkflowManager *) = [](QObject *) { return true; };
void (*WirelessWorkflowManager__connectWirelessSilently)(WirelessWorkflowManager *) = [](QObject *) {};
void (*WirelessWorkflowManager__connectWireless)(WirelessWorkflowManager *, bool, bool) = [](QObject *, bool, bool) {};
int (*ReadingView__getCalculatedReadProgress)(QWidget *) = [](QWidget *) { return 43; };
QString getDeviceModel() { return "Test"; }
PowerTimer *construct_PowerTimer(const QString &, QObject *parent) { return new QObject(parent); }
int (*PowerTimer__timeRemaining)(PowerTimer *) = [](QObject *) { return 1000; };
void (*PowerTimer__fireAt)(PowerTimer *, const QDateTime &) = [](QObject *, const QDateTime &) {};
ConfirmationDialog *(*ConfirmationDialogFactory__getConfirmationDialog)(QWidget *) = [](QWidget *parent) {
  ++dialogs;
  return new QDialog(parent);
};
void (*ConfirmationDialog__showCloseButton)(ConfirmationDialog *, bool) = [](QDialog *, bool) {};
void (*ConfirmationDialog__setText)(ConfirmationDialog *, const QString &) = [](QDialog *, const QString &) {};
void (*ConfirmationDialog__setTitle)(ConfirmationDialog *, const QString &) = [](QDialog *, const QString &title) {
  errorTitle = title;
};
void (*ConfirmationDialog__setAcceptButtonText)(ConfirmationDialog *, const QString &) = [](QDialog *,
                                                                                            const QString &) {};
void (*ConfirmationDialog__setRejectButtonText)(ConfirmationDialog *, const QString &) = [](QDialog *,
                                                                                            const QString &) {};
void (*ConfirmationDialogFactory__showErrorDialog)(const QString &, const QString &) = [](const QString &title,
                                                                                          const QString &) {
  ++dialogs;
  errorTitle = title;
};
QString describeBookError(const QString &, const QString &message) { return message; }
void showBookErrorDialog(const QString &, const QString &) {
  ++dialogs;
  errorTitle = Settings::getInstance()->serviceName();
}

static void waitFor(const std::function<bool()> &done) {
  QElapsedTimer timer;
  timer.start();
  while (!done() && timer.elapsed() < 2000) {
    QApplication::processEvents();
    QThread::msleep(1);
  }
  assert(done());
}
static void executable(const QString &code, bool executable = true) {
  QFile file(Files::cli);
  assert(file.open(QIODevice::WriteOnly | QIODevice::Truncate));
  file.write(code.toUtf8());
  file.close();
  file.setPermissions(QFile::ReadOwner | QFile::WriteOwner | (executable ? QFile::ExeOwner : QFile::Permissions()));
}
int main(int argc, char **argv) {
  QApplication app(argc, argv);
  QWidget window;
  view = &window;
  QDir().mkpath(Files::adds_directory);
  QSettings config(Files::config, QSettings::IniFormat);
  config.setValue("provider", "dual");
  config.setValue("access_token", "test");
  config.setValue("readwise_token", "test");
  config.setValue("retry_on_network", true);
  config.sync();
  const QString scenario = argc > 1 ? argv[1] : "missing";
  if (scenario == "network-retry") {
    executable("#!/bin/sh\ncase \"$*\" in\n*hardcover*) echo '{\"kind\":\"Error\","
               "\"error_code\":\"Network\",\"message\":\"Unavailable\"}'; exit 1;;\nesac\n");
    SyncQueue hardcover(false), readwise(true);
    hardcover.updateReadProgress("retry");
    readwise.updateReadProgress("retry");
    hardcover.run("retry");
    readwise.run("retry");
    waitFor([&] { return !hardcover.running && !readwise.running; });
    assert(hardcover.failed && !readwise.failed);
    assert(readwise.getProgress().isEmpty());
    assert(hardcover.getProgress().value("retry") == 43);
    assert(Settings::getInstance()->getLastProgress("retry", false) == 0);
    assert(Settings::getInstance()->getLastProgress("retry", true) == 43);
    assert(dialogs == 0);
    executable("#!/bin/sh\nexit 0\n");
    wireless.networkConnected();
    assert(hardcover.running && !readwise.running);
    waitFor([&] { return !hardcover.running; });
    assert(!hardcover.failed);
    assert(Settings::getInstance()->getLastProgress("retry", false) == 43);
  } else if (scenario == "missing" || scenario == "not-executable") {
    if (scenario == "not-executable")
      executable("#!/bin/sh\nexit 0\n", false);
    SyncQueue queue(true);
    queue.updateReadProgress("book");
    queue.run("book");
    waitFor([&] { return !queue.running; });
    assert(queue.failed);
    executable("#!/bin/sh\nexit 0\n");
    queue.run("book");
    waitFor([&] { return !queue.running; });
    assert(!queue.failed);
  } else {
    const bool silent = scenario == "silent";
    const QString error = argc > 2 ? argv[2] : "ReadwiseSync";
    if (error == "crash")
      executable("#!/bin/sh\nkill -SEGV $$\n");
    else if (error == "unexpected-exit")
      executable("#!/bin/sh\nexit 12\n");
    else
      executable("#!/bin/sh\necho '{\"kind\":\"Error\",\"error_code\":\"" + error +
                 "\",\"message\":\"Test failure\"}'\nexit 1\n");
    int failures = 0;
    CLI::FailureReason reason = CLI::Error;
    CLI *cli = CLI::update("book", 43, silent, true);
    QObject::connect(cli, &CLI::failure, [&](CLI::FailureReason value) {
      ++failures;
      reason = value;
    });
    waitFor([&] { return failures > 0; });
    assert(failures == 1);
    if (error == "Network")
      assert(reason == CLI::Network);
    if (silent) {
      assert(dialogs == 0);
      for (QWidget *widget : QApplication::topLevelWidgets())
        assert(!qobject_cast<QDialog *>(widget) || !widget->isVisible());
    } else {
      assert(dialogs == 1);
      assert(errorTitle == "Readwise Books");
    }
  }
  std::cout << qPrintable(scenario) << " QProcess OK\n";
}
#include "cliprocess.moc"

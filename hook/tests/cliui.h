#pragma once
#include <QDialog>
#include <QString>

// Only Nickel dialogs are replaced; CLI and QProcess are real in this test.
class SignInDialog {
public:
  static void show(bool) { (new QDialog)->open(); }
};
class SearchDialog {
public:
  static void show(const QString &) { (new QDialog)->open(); }
};

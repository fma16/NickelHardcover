#pragma once

#include "../messages.h"
#include "../settings.h"
#include "../widgets/dialog.h"

class SignInDialog : public Dialog {
  Q_OBJECT

public:
  static void show() { show(Settings::getInstance()->isReadwise()); }
  static void show(bool readwise) { new SignInDialog(readwise); }

public Q_SLOTS:
  void continueTapped();

private:
  SignInDialog(bool readwise);

  void response(const Messages &message);

  QImage buildQrCode(const QString &text);

  QString deviceCode;
};

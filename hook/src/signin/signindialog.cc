#include <QImage>
#include <QPixmap>
#include <QVBoxLayout>

#include <NickelHook.h>
#include <qrcodegen.h>

#include "../cli.h"
#include "../settings.h"
#include "../widgets/label.h"
#include "qboxlayout.h"
#include "qnamespace.h"
#include "signindialog.h"

SignInDialog::SignInDialog(bool readwise)
    : Dialog(readwise ? "Sign in to Readwise Books" : "Sign in to Hardcover.app") {
  QVBoxLayout *layout = new QVBoxLayout(this);
  layout->setContentsMargins(0, 0, 0, 0);

  if (readwise) {
    buildReadwiseHelp(layout);
    return;
  }

  Label *status = new Label(Label::Small, "Loading. Please wait...");
  layout->addWidget(status, 1, Qt::AlignCenter);

  CLI *cli = CLI::oauthRequest();
  QObject::connect(cli, &CLI::response, this, &SignInDialog::response);
  QObject::connect(cli, &CLI::failure, this, &SignInDialog::closeDialog);
}

void SignInDialog::buildReadwiseHelp(QVBoxLayout *layout) {
  QFrame *frame = new QFrame(this);
  frame->setObjectName("readwiseHelp");
  // Side margins match the Settings dialog; gaps between steps scale with the device.
  frame->setStyleSheet(R"(
    [qApp_deviceIsTrilogy=true] #readwiseHelp { margin: 0 36px; }
    [qApp_deviceIsPhoenix=true] #readwiseHelp { margin: 0 48px; }
    [qApp_deviceIsDragon=true] #readwiseHelp { margin: 0 66px; }
    [qApp_deviceIsStorm=true] #readwiseHelp { margin: 0 75px; }
    [qApp_deviceIsDaylight=true] #readwiseHelp { margin: 0 84px; }

    [qApp_deviceIsTrilogy=true] Label[gap=true] { padding-bottom: 18px; }
    [qApp_deviceIsPhoenix=true] Label[gap=true] { padding-bottom: 24px; }
    [qApp_deviceIsDragon=true] Label[gap=true] { padding-bottom: 33px; }
    [qApp_deviceIsStorm=true] Label[gap=true] { padding-bottom: 38px; }
    [qApp_deviceIsDaylight=true] Label[gap=true] { padding-bottom: 42px; }
  )");

  QVBoxLayout *box = new QVBoxLayout(frame);
  box->setContentsMargins(0, 0, 0, 0);
  box->setSpacing(0);

  Label *intro = new Label(Label::Medium, "Readwise Books uses a personal access token instead of a QR code.");
  intro->setWordWrap(true);
  intro->setProperty("gap", true);
  box->addWidget(intro);

  QStringList steps = {
      "On your computer, open <b>readwise.io/access_token</b> and copy your token.",
      "Connect the Kobo by USB and open <b>.adds/NickelHardcover/config.ini</b>.",
      "Set <b>readwise_token&nbsp;=&nbsp;</b><i>your token</i> on its own line, without a leading <b>;</b>",
      "Eject the Kobo, then reopen your book.",
  };
  for (int i = 0; i < steps.size(); i++) {
    QHBoxLayout *row = new QHBoxLayout();
    row->setContentsMargins(0, 0, 0, 0);

    Label *number = new Label(Label::Medium, QString("<b>%1.</b>").arg(i + 1));
    number->setProperty("gap", true);
    row->addWidget(number, 0, Qt::AlignTop);

    Label *text = new Label(Label::Medium, steps[i]);
    text->setWordWrap(true);
    text->setProperty("gap", true);
    row->addWidget(text, 1, Qt::AlignTop);

    box->addLayout(row);
  }

  Label *note = new Label(Label::Small, "Keep this token private: it gives access to your Readwise account.");
  note->setWordWrap(true);
  note->setProperty("style", "italic");
  note->setProperty("gap", true);
  box->addWidget(note);

  N3ButtonLabel *button = construct_N3ButtonLabel(this);
  button->setProperty("primaryButton", true);
  button->setText("Done");
  box->addWidget(button, 0, Qt::AlignCenter);
  QObject::connect(button, SIGNAL(tapped(bool)), dialog, SLOT(deleteLater()));

  layout->addStretch(1);
  layout->addWidget(frame);
  layout->addStretch(2);
}

void SignInDialog::response(const Messages &message) {
  if (!message.isOAuthDevice()) {
    ConfirmationDialogFactory__showErrorDialog("Hardcover.app", "Unexpected CLI response for <i>oauthRequest</i>");
    return;
  }

  deviceCode = message.o_auth_device->device_code;

  QVBoxLayout *box = qobject_cast<QVBoxLayout *>(layout());
  box->itemAt(0)->widget()->deleteLater();

  box->addStretch(1);

  QLabel *qrLabel = new QLabel(this);
  qrLabel->setPixmap(QPixmap::fromImage(buildQrCode(message.o_auth_device->verification_uri_complete)));
  box->addWidget(qrLabel, 0, Qt::AlignCenter);

  QString text = QString("<br>Please scan this QR code<br><br>— or —<br><br>Visit <b>%1</b> and enter<br>")
                     .arg(message.o_auth_device->verification_uri);
  QLabel *label = new Label(Label::Medium, text);
  label->setAlignment(Qt::AlignCenter);
  box->addWidget(label, 0, Qt::AlignCenter);

  box->addWidget(new Label(Label::AvenirExtraLarge, message.o_auth_device->user_code + "<br>"), 0, Qt::AlignCenter);

  N3ButtonLabel *button = construct_N3ButtonLabel(this);
  button->setProperty("primaryButton", true);
  button->setText("Continue");
  box->addWidget(button, 0, Qt::AlignCenter);
  QObject::connect(button, SIGNAL(tapped(bool)), this, SLOT(continueTapped()));

  box->addStretch(1);
}

QImage SignInDialog::buildQrCode(const QString &text) {
  constexpr int maxVersion = 4;
  constexpr int scale = 16;
  constexpr size_t bufLen = qrcodegen_BUFFER_LEN_FOR_VERSION(maxVersion);

  uint8_t qrcode[bufLen];
  uint8_t tempBuffer[bufLen];

  QByteArray utf8 = text.toUtf8();

  bool ok = qrcodegen_encodeText(utf8.constData(), tempBuffer, qrcode, qrcodegen_Ecc_MEDIUM, qrcodegen_VERSION_MIN,
                                 maxVersion, qrcodegen_Mask_AUTO, true);

  if (!ok) {
    return QImage();
  }

  int size = qrcodegen_getSize(qrcode);
  int imgSize = size * scale;

  QImage image(imgSize, imgSize, QImage::Format_RGB32);
  image.fill(Qt::white);

  for (int y = 0; y < size; y++) {
    for (int x = 0; x < size; x++) {
      if (qrcodegen_getModule(qrcode, x, y)) {
        for (int dy = 0; dy < scale; dy++) {
          for (int dx = 0; dx < scale; dx++) {
            image.setPixel(x * scale + dx, y * scale + dy, qRgb(0, 0, 0));
          }
        }
      }
    }
  }
  return image;
}

void SignInDialog::continueTapped() {
  CLI *cli = CLI::oauthSet(deviceCode);
  QObject::connect(cli, &CLI::success, this, &SignInDialog::closeDialog);
  QObject::connect(cli, &CLI::failure, this, &SignInDialog::closeDialog);
}

#include "../src/cover.h"

#include <QApplication>
#include <QBuffer>
#include <QEventLoop>
#include <QNetworkReply>
#include <QPointer>
#include <QTimer>
#include <cassert>
#include <cstring>
#include <iostream>

struct Response {
  QByteArray body;
  QUrl redirect;
  qint64 length = -1;
  bool hang = false;
};

class Reply : public QNetworkReply {
public:
  bool aborted = false;
  Reply(const QNetworkRequest &request, const Response &response, QObject *parent)
      : QNetworkReply(parent), body(response.body) {
    setRequest(request);
    setUrl(request.url());
    setAttribute(QNetworkRequest::HttpStatusCodeAttribute, response.redirect.isEmpty() ? 200 : 302);
    if (!response.redirect.isEmpty())
      setAttribute(QNetworkRequest::RedirectionTargetAttribute, response.redirect);
    if (response.length >= 0)
      setHeader(QNetworkRequest::ContentLengthHeader, response.length);
    open(QIODevice::ReadOnly);
    if (!response.hang)
      QTimer::singleShot(0, this, [this] {
        emit metaDataChanged();
        if (aborted)
          return;
        emit readyRead();
        if (aborted)
          return;
        setFinished(true);
        emit finished();
      });
  }
  void abort() override {
    aborted = true;
    setError(OperationCanceledError, "Cancelled");
    setFinished(true);
    emit finished();
  }
  qint64 bytesAvailable() const override { return body.size() - offset + QNetworkReply::bytesAvailable(); }

protected:
  qint64 readData(char *data, qint64 max) override {
    const qint64 count = qMin(max, qint64(body.size()) - offset);
    if (!count)
      return -1;
    std::memcpy(data, body.constData() + offset, count);
    offset += count;
    return count;
  }

private:
  QByteArray body;
  qint64 offset = 0;
};

class Network : public QNetworkAccessManager {
public:
  QList<Response> responses;
  QList<QUrl> requested;
  QPointer<Reply> last;

protected:
  QNetworkReply *createRequest(Operation operation, const QNetworkRequest &request, QIODevice *) override {
    assert(operation == GetOperation);
    assert(!request.hasRawHeader("Authorization") && !request.hasRawHeader("Cookie"));
    assert(request.attribute(QNetworkRequest::CookieLoadControlAttribute) == QNetworkRequest::Manual);
    assert(request.attribute(QNetworkRequest::CookieSaveControlAttribute) == QNetworkRequest::Manual);
    assert(request.attribute(QNetworkRequest::AuthenticationReuseAttribute) == QNetworkRequest::Manual);
    assert(request.attribute(QNetworkRequest::RedirectPolicyAttribute) == QNetworkRequest::ManualRedirectPolicy);
    requested.append(request.url());
    assert(!responses.isEmpty());
    last = new Reply(request, responses.takeFirst(), this);
    return last;
  }
};

QByteArray raster(const QSize &size = QSize(120, 180), const char *format = "PNG") {
  QImage image(size, QImage::Format_RGB32);
  image.fill(QColor("#247583"));
  QByteArray bytes;
  QBuffer buffer(&bytes);
  buffer.open(QIODevice::WriteOnly);
  assert(image.save(&buffer, format));
  return bytes;
}

void settle() {
  QEventLoop loop;
  QTimer::singleShot(25, &loop, &QEventLoop::quit);
  loop.exec();
}

QPixmap pixmap(const QLabel &label) { return label.pixmap(Qt::ReturnByValue); }

int main(int argc, char **argv) {
  QApplication app(argc, argv);
  const QUrl cover("https://assets.hardcover.app/cover.jpg");
  const QUrl bookwise("https://bookwise.com/product/cover/123/test");
  const QUrl cdn("https://dfzgmkfntut7m.cloudfront.net/media/test.jpg");
  const QList<QUrl> forbidden = {QUrl("file:///tmp/private.png"),
                                 QUrl("http://assets.hardcover.app/cover.jpg"),
                                 QUrl("https://127.0.0.1/cover"),
                                 QUrl("https://[::1]/cover"),
                                 QUrl("https://192.168.1.1/cover"),
                                 QUrl("https://assets.hardcover.app.attacker.example/cover"),
                                 QUrl("https://attacker.example/cover"),
                                 QUrl("https://assets.hardcover.app:8443/cover"),
                                 QUrl("https://user:password@assets.hardcover.app/cover"),
                                 QUrl("https://bookwise.com/other"),
                                 QUrl("data:image/png;base64,AAAA"),
                                 QUrl("//assets.hardcover.app/cover")};
  for (const QUrl &url : forbidden) {
    Network network;
    QLabel label;
    loadCover(&label, &network, url);
    settle();
    assert(network.requested.isEmpty());
    assert(label.property("blank").toBool() && pixmap(label).isNull());
    if (url.isRelative())
      continue; // Relative redirect URLs are resolved, unlike initial URLs.
    Response redirect;
    redirect.redirect = url;
    network.responses.append(redirect);
    loadCover(&label, &network, cover);
    settle();
    assert(network.requested.size() == 1);
    assert(label.property("blank").toBool() && pixmap(label).isNull());
  }
  for (const char *format : {"PNG", "JPEG"}) {
    Network network;
    Response redirect, image;
    redirect.redirect = cdn;
    image.body = raster(QSize(600, 900), format);
    network.responses = {redirect, image};
    QLabel label;
    loadCover(&label, &network, bookwise);
    settle();
    assert(!label.property("blank").toBool());
    assert(pixmap(label).size() == QSize(256, 384));
    assert(network.requested == QList<QUrl>({bookwise, cdn}));
  }
  for (int extra : {0, 1}) {
    for (qint64 length : {-1, 1, 4 * 1024 * 1024 + 1}) {
      Network network;
      Response image;
      image.body = raster();
      image.body.append(QByteArray(4 * 1024 * 1024 + extra - image.body.size(), ' '));
      image.length = length;
      network.responses.append(image);
      QLabel label;
      loadCover(&label, &network, cover);
      settle();
      const bool rejected = extra == 1 || length > 4 * 1024 * 1024;
      assert(label.property("blank").toBool() == rejected);
      assert(pixmap(label).isNull() == rejected);
    }
    // Redirect responses consume the same byte budget as the final image.
    Network network;
    Response redirect, image;
    redirect.redirect = cdn;
    redirect.body = QByteArray(1024 * 1024, ' ');
    image.body = raster();
    image.body.append(QByteArray(3 * 1024 * 1024 + extra - image.body.size(), ' '));
    network.responses = {redirect, image};
    QLabel label;
    loadCover(&label, &network, bookwise);
    settle();
    assert(label.property("blank").toBool() == (extra == 1));
    assert(pixmap(label).isNull() == (extra == 1));
  }
  for (const QByteArray &body : {raster(QSize(2048, 2048)), raster(QSize(2049, 2048)), raster(QSize(4097, 1)),
                                 QByteArray("<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'/>"),
                                 QByteArray("\x89PNG\r\n\x1a\n")}) {
    Network network;
    Response image;
    image.body = body;
    network.responses.append(image);
    QLabel label;
    loadCover(&label, &network, cover);
    settle();
    const bool accepted = body == raster(QSize(2048, 2048));
    assert(label.property("blank").toBool() != accepted);
    assert(pixmap(label).isNull() != accepted);
  }
  {
    Network network;
    Response redirect;
    redirect.redirect = QUrl("/cover.jpg");
    network.responses = {redirect, redirect, redirect, redirect};
    QLabel label;
    loadCover(&label, &network, cover);
    settle();
    assert(network.requested.size() == 4 && label.property("blank").toBool());
  }
  {
    Network network;
    Response response;
    response.hang = true;
    network.responses.append(response);
    QLabel *label = new QLabel;
    loadCover(label, &network, cover);
    delete label;
    assert(network.last && network.last->aborted);
    settle();
    assert(!network.last);
  }
  std::cout << "Cover security: URL/redirect policy, byte/pixel limits, raster decoding and cancellation OK\n";
}

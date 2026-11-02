#include "cover.h"
#include "files.h"

#include <QBuffer>
#include <QImageReader>
#include <QNetworkReply>
#include <QTimer>

namespace {
constexpr qint64 maxBytes = 4 * 1024 * 1024;
constexpr qint64 maxPixels = 4 * 1024 * 1024;

bool allowed(const QUrl &url) {
  if (!url.isValid() || url.scheme() != "https" || !url.userInfo().isEmpty() ||
      (url.port(-1) != -1 && url.port() != 443))
    return false;
  // Exact hosts only: no arbitrary subdomains, IP addresses, or generic CDN
  // suffixes. Bookwise's documented cover endpoint redirects to its own CDN.
  const QString host = url.host().toLower();
  if (host == "bookwise.com")
    return url.path().startsWith("/product/cover/");
  return host == "assets.hardcover.app" || host == "dfzgmkfntut7m.cloudfront.net" ||
         host == "d34adp677peecb.cloudfront.net" || host == "m.media-amazon.com" ||
         host == "images-na.ssl-images-amazon.com" || host == "i.gr-assets.com";
}

class CoverRequest : public QObject {
public:
  CoverRequest(QLabel *label, QNetworkAccessManager *network, const QUrl &url)
      : QObject(label), label(label), network(network) {
    label->setProperty("blank", false);
    label->setPixmap(QPixmap(Files::loading_cover));
    QTimer *timer = new QTimer(this);
    timer->setSingleShot(true);
    connect(timer, &QTimer::timeout, this, [this] { fail(); });
    timer->start(30000); // One deadline for the whole redirect chain.
    fetch(url);
  }

  ~CoverRequest() { cancel(); }

private:
  QLabel *label;
  QNetworkAccessManager *network;
  QNetworkReply *reply = nullptr;
  QByteArray bytes;
  qint64 received = 0;
  int redirects = 0;

  void cancel() {
    if (reply) {
      reply->disconnect(this);
      if (!reply->isFinished())
        reply->abort();
      reply->deleteLater();
      reply = nullptr;
    }
  }

  void fail() {
    cancel();
    label->clear();
    label->setProperty("blank", true);
    deleteLater();
  }

  void fetch(const QUrl &url) {
    if (!allowed(url)) {
      fail();
      return;
    }
    QNetworkRequest request(url);
#if QT_VERSION >= QT_VERSION_CHECK(5, 9, 0)
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::ManualRedirectPolicy);
#endif
    // Qt 5.2 (Kobo) does not follow redirects automatically.
    request.setAttribute(QNetworkRequest::CookieLoadControlAttribute, QNetworkRequest::Manual);
    request.setAttribute(QNetworkRequest::CookieSaveControlAttribute, QNetworkRequest::Manual);
    request.setAttribute(QNetworkRequest::AuthenticationReuseAttribute, QNetworkRequest::Manual);
    reply = network->get(request);
    reply->setReadBufferSize(64 * 1024);
    connect(reply, &QNetworkReply::metaDataChanged, this, [this] {
      // Content-Length describes this whole response, including bytes already read.
      if (reply->header(QNetworkRequest::ContentLengthHeader).toLongLong() > maxBytes - (received - bytes.size()))
        fail();
    });
    connect(reply, &QNetworkReply::readyRead, this, [this] { read(); });
    connect(reply, &QNetworkReply::finished, this, [this] { finish(); });
  }

  bool read() {
    const QByteArray chunk = reply->read(maxBytes - received + 1);
    received += chunk.size();
    if (received > maxBytes) {
      fail();
      return false;
    }
    bytes.append(chunk);
    return true;
  }

  void finish() {
    if (reply->error() != QNetworkReply::NoError ||
        reply->header(QNetworkRequest::ContentLengthHeader).toLongLong() > maxBytes || !read()) {
      fail();
      return;
    }
    const int status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    const QUrl location = reply->attribute(QNetworkRequest::RedirectionTargetAttribute).toUrl();
    if (!location.isEmpty() && (status == 301 || status == 302 || status == 303 || status == 307 || status == 308)) {
      const QUrl next = reply->url().resolved(location);
      cancel();
      bytes.clear();
      if (++redirects > 3) {
        fail();
        return;
      }
      fetch(next); // Revalidate every hop, including HTTPS downgrades.
      return;
    }
    if (status != 200) {
      fail();
      return;
    }
    // Select a known raster decoder before asking Qt to inspect the file.
    // Autodetection would also invoke installed SVG and other image plugins.
    const QByteArray format = bytes.startsWith("\x89PNG\r\n\x1a\n") ? "png"
                              : bytes.startsWith("\xff\xd8\xff")    ? "jpeg"
                                                                    : "";
    if (format.isEmpty()) {
      fail();
      return;
    }
    QBuffer buffer(&bytes);
    buffer.open(QIODevice::ReadOnly);
    QImageReader reader(&buffer, format);
    reader.setAutoDetectImageFormat(false);
    const QSize size = reader.size();
    // Do not invoke SVG/other plugins or decode a giant raster before scaling.
    if (size.width() <= 0 || size.height() <= 0 || size.width() > 4096 || size.height() > 4096 ||
        qint64(size.width()) * size.height() > maxPixels) {
      fail();
      return;
    }
    if (size.width() > 256 || size.height() > 384)
      reader.setScaledSize(size.scaled(QSize(256, 384), Qt::KeepAspectRatio));
    const QImage image = reader.read();
    if (image.isNull()) {
      fail();
      return;
    }
    label->setPixmap(QPixmap::fromImage(image));
    cancel();
    deleteLater();
  }
};
} // namespace

void loadCover(QLabel *label, QNetworkAccessManager *network, const QUrl &url) {
  new CoverRequest(label, network, url);
}

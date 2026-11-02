#pragma once

#include <QLabel>
#include <QNetworkAccessManager>
#include <QUrl>

// Fetch a bounded raster thumbnail from the providers' cover hosts. Failure
// leaves a blank cover; the book row remains usable. Destroying the label cancels.
void loadCover(QLabel *label, QNetworkAccessManager *network, const QUrl &url);

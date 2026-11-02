#pragma once

#include <QDateTime>
#include <QObject>
#include <QSettings>
#include <QVariant>

#include "files.h"

class Settings : public QObject {
  Q_OBJECT

public:
  static Settings *getInstance();

  void setEnabled(const QString &contentId, bool value) { setEnabled(contentId, value, isReadwise()); }
  void setEnabled(const QString &contentId, bool value, bool readwise) {
    setValue(contentId, "enabled", value, readwise);
  }
  bool isEnabled(const QString &contentId) const { return isEnabled(contentId, isReadwise()); }
  bool isEnabled(const QString &contentId, bool readwise) const;

  void setLinkedId(const QString &contentId, const QString &value) {
    setValue(contentId, "linkedbook", value, isReadwise());
  }
  QString getLinkedId(const QString &contentId) const { return getLinkedId(contentId, isReadwise()); }
  QString getLinkedId(const QString &contentId, bool readwise) const {
    return getValue(contentId, "linkedbook", readwise).toString();
  }

  void setLastProgress(const QString &contentId, int value, bool readwise) {
    setValue(contentId, "progress", value, readwise);
  }
  int getLastProgress(const QString &contentId) const { return getLastProgress(contentId, isReadwise()); }
  int getLastProgress(const QString &contentId, bool readwise) const {
    return getValue(contentId, "progress", readwise).toInt();
  }

  // In dual mode the book-management UI retains the complete Hardcover feature set.
  bool isReadwise() const { return config->value("provider", "hardcover").toString() == "readwise"; }
  bool isDual() const { return config->value("provider").toString() == "dual"; }
  bool usesService(bool readwise) const { return isDual() || isReadwise() == readwise; }
  QString serviceName() const { return isReadwise() ? "Readwise Books" : "Hardcover.app"; }
  bool publishReadwiseSessions() const { return config->value("readwise_publish_to_feed", true).toBool(); }
  void clearAccessToken() { clearAccessToken(isReadwise()); }
  void clearAccessToken(bool readwise) { config->setValue(readwise ? "readwise_token" : "access_token", ""); }
  bool isSignedIn() const { return isSignedIn(isReadwise()); }
  bool isSignedIn(bool readwise) const {
    return !config->value(readwise ? "readwise_token" : "access_token").toString().isEmpty();
  }

  void setAutoSyncDefault(bool value) { config->setValue("auto_sync_default", value); }
  bool getAutoSyncDefault() const { return config->value("auto_sync_default", false).toBool(); }

  void setDebug(bool value);
  bool getDebug() const { return config->value("debug").toBool(); }

  void setJournalPrivacy(const QVariant &value) { config->setValue("journal_privacy", value); }
  QString getJournalPrivacy() const { return config->value("journal_privacy", "account").toString().toLower(); }

  void setRetryOnNetwork(bool value) { config->setValue("retry_on_network", value); }
  bool getRetryOnNetwork() const { return config->value("retry_on_network", false).toBool(); }

  void setSyncAnnotations(bool value) { config->setValue("sync_annotations", value); }
  bool getSyncAnnotations() const { return config->value("sync_annotations", false).toBool(); }

  void setSyncOnClose(const QVariant &value) { config->setValue("sync_on_close", value); }
  int getSyncOnClose() const;

  void setSyncOnRead(const QVariant &value) { config->setValue("sync_on_read", value); }
  int getSyncOnRead() const;

  void setSyncOnSchedule(const QVariant &value) { config->setValue("sync_on_schedule", value); }
  int getSyncOnSchedule() const;

  bool is24HourClock() const { return kobo->value("ApplicationPreferences/is24HourClock").toBool(); }

  void deleteConfig();

private:
  Settings(QObject *parent = nullptr);

  void currentViewChanged(const QString &name);

  QSettings *library = nullptr;
  QSettings *config = nullptr;
  QSettings *kobo = nullptr;

  QString getPath(QString contentId, const QString &key, bool readwise) const;
  void setValue(const QString &contentId, const QString &key, const QVariant &value, bool readwise);
  QVariant getValue(const QString &contentId, const QString &key, bool readwise,
                    const QVariant &defaultValue = QVariant()) const;
};

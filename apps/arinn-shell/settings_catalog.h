/*
 * SPDX-FileCopyrightText: 2026 Arinn contributors
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#pragma once
#include <QHash>
#include <QObject>
#include <QSet>
#include <QTimer>
#include <QVariantList>
#include <memory>
#include <QtGlobal>

// Whether a hardware keyboard is attached. Arrow keys and other remote
// navigation do not count. The string editor uses this for the no-keyboard
// state; the first character typed there enables the field on its own.
class KeyboardMonitor : public QObject {
  Q_OBJECT
  Q_PROPERTY(bool available READ available NOTIFY availableChanged)
public:
  explicit KeyboardMonitor(QObject *parent = nullptr);
  bool available() const { return m_available; }
signals:
  void availableChanged();

private:
  void scan();
  bool m_available = false;
};

// Reads org.arinn.Settings1 for the shell. The screen sees categories and
// typed rows only. SetValue is an operation; provider values are not logged.
// For a library provider that lists LibraryRoots1 it also reads
// Library1.GetRoots (ADR-0071), so a multiple path row carries the folders
// that provider reports unavailable (spec §5.6).
class SettingsCatalog : public QObject {
  Q_OBJECT
  Q_PROPERTY(QString screenState READ screenState NOTIFY catalogChanged)
  Q_PROPERTY(QVariantList categories READ categories NOTIFY catalogChanged)
  Q_PROPERTY(QVariantList rows READ rows NOTIFY catalogChanged)
  // The shell's own preferences from the daemon's `arinn` settings
  // (ADR-0112). Defaults until the daemon has answered; the last known value
  // while it is away.
  Q_PROPERTY(bool lightField READ lightField NOTIFY preferencesChanged)
  Q_PROPERTY(int ambientTimeoutMinutes READ ambientTimeoutMinutes NOTIFY
                 preferencesChanged)
public:
  explicit SettingsCatalog(QObject *parent = nullptr);
  QString screenState() const;
  QVariantList categories() const { return m_categories; }
  QVariantList rows() const { return m_rows; }
  bool lightField() const { return m_lightField; }
  int ambientTimeoutMinutes() const { return m_ambientMinutes; }
  Q_INVOKABLE void setValue(const QString &providerId, const QString &settingId,
                            const QVariant &value);
  Q_INVOKABLE void dismissError(const QString &providerId,
                                const QString &settingId);
public slots:
  void refresh();
  void onSettingsChanged(const QString &providerId);
  void refreshRoots();
  void onStateChanged(const QString &providerId, const QString &capability,
                      const QString &generation, qulonglong revision);
signals:
  void catalogChanged();
  void preferencesChanged();

private:
  struct SettingRecord {
    QString id;
    QString category;
    QString title;
    QString description;
    QString kind;
    QString valueJson;
    QString constraints;
    bool enabled = false;
  };
  struct ProviderRecord {
    QString id;
    QString name;
    QString generation;
    QList<SettingRecord> settings;
    bool ok = false;
  };
  struct Load;
  struct RootScan;
  void readLibraryProviders(const std::shared_ptr<RootScan> &scan,
                            const QString &token);
  void readRoots(const QString &providerId);
  void readRootList(const QString &providerId, const QString &objectPath,
                    quint64 ticket, quint64 epoch);
  void finishRootRead(const QString &providerId);
  void dropRoots(const QString &providerId);
  QString keyFor(const QString &providerId, const QString &settingId) const;
  const SettingRecord *find(const QString &providerId,
                            const QString &settingId) const;
  QString encodeValue(const ProviderRecord &provider, const SettingRecord &row,
                      const QVariant &value) const;
  void readProvider(const std::shared_ptr<Load> &load, int index);
  void finishLoad(const std::shared_ptr<Load> &load);
  void publish();
  void watchOperation(const QString &key, const QString &objectPath,
                      quint64 ticket, quint64 epoch);
  void settle(const QString &key, quint64 ticket, quint64 epoch,
              const QString &error, bool deferred = false);
  void syncDeferred();
  void readPreferences();
  bool m_lightField = true;
  int m_ambientMinutes = 10;
  QVariantList m_categories;
  QVariantList m_rows;
  // The screen state the last catalogChanged announced (#441).
  QString m_publishedState;
  QList<ProviderRecord> m_providers;
  QSet<QString> m_pending;
  QSet<QString> m_deferred;
  QHash<QString, QString> m_errors;
  QHash<QString, QString> m_deferredGeneration;
  QHash<QString, QString> m_saveGeneration;
  QHash<QString, quint64> m_tickets;
  // Library providers: id → the daemon path that serves its Library1.
  QHash<QString, QString> m_libraryPaths;
  // Root availability per library provider (path → available), only for a
  // provider whose GetRoots answered. No entry means no unavailable state.
  QHash<QString, QHash<QString, bool>> m_roots;
  QHash<QString, quint64> m_rootTickets;
  // Library provider generations from Daemon1.Providers, and whether
  // Features listed LibraryRoots1 in that generation.
  QHash<QString, QString> m_libraryGenerations;
  QHash<QString, QPair<QString, bool>> m_rootFeatures;
  // One roots read per provider at a time; a trigger meanwhile runs it once
  // more when it ends.
  QSet<QString> m_rootsReading;
  QSet<QString> m_rootsAgain;
  quint64 m_rootScan = 0;
  bool m_loaded = false;
  bool m_error = false;
  bool m_unreachable = false;
  bool m_refreshing = false;
  bool m_dirty = false;
  quint64 m_epoch = 0;
  QTimer m_timer;
};

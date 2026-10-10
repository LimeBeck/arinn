/*
 * SPDX-FileCopyrightText: 2026 Arinn contributors
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#pragma once
#include "flow_trace.h"
#include <QDBusMessage>
#include <QHash>
#include <QObject>
#include <QSet>
#include <QTimer>
#include <QVariantList>
#include <memory>
class Backend : public QObject {
  Q_OBJECT
  Q_CLASSINFO("D-Bus Interface", "org.arinn.Shell1")
  Q_PROPERTY(bool connected READ connected NOTIFY changed)
  Q_PROPERTY(bool available READ available NOTIFY changed)
  Q_PROPERTY(bool homeVisible READ homeVisible NOTIFY changed)
  Q_PROPERTY(QString daemonEpoch READ daemonEpoch NOTIFY changed)
  Q_PROPERTY(QString returnWindow READ returnWindow NOTIFY changed)
  Q_PROPERTY(QVariantList items READ items NOTIFY itemsChanged)
  Q_PROPERTY(QVariantList continueEntries READ continueEntries NOTIFY
                 continueEntriesChanged)
  Q_PROPERTY(
      QVariantList applications READ applications NOTIFY applicationsChanged)
  Q_PROPERTY(
      QVariantList webShortcuts READ webShortcuts NOTIFY webShortcutsChanged)
  Q_PROPERTY(QVariantList favorites READ favorites NOTIFY changed)
  Q_PROPERTY(bool favoritesReady READ favoritesReady NOTIFY changed)
  Q_PROPERTY(bool favoriteSaving READ favoriteSaving NOTIFY changed)
  Q_PROPERTY(QString favoriteError READ favoriteError NOTIFY changed)
  Q_PROPERTY(QString homeState READ homeState NOTIFY changed)
  // The daemon runs another build than this shell (ADR-0097).
  Q_PROPERTY(bool buildMismatch READ buildMismatch NOTIFY changed)
  Q_PROPERTY(QString applicationsState READ applicationsState NOTIFY changed)
  Q_PROPERTY(QStringList busyEntryIds READ busyEntryIds NOTIFY changed)
  Q_PROPERTY(QString message READ message NOTIFY changed)
  Q_PROPERTY(QVariantMap shortcutEditor READ shortcutEditor NOTIFY changed)
  Q_PROPERTY(bool shortcutSaving READ shortcutSaving NOTIFY changed)
  Q_PROPERTY(QString shortcutError READ shortcutError NOTIFY changed)
public:
  explicit Backend(QObject *parent = nullptr);
  bool connected() const { return m_connected; }
  bool available() const { return m_available; }
  bool homeVisible() const { return m_home; }
  QString daemonEpoch() const { return m_daemonEpoch; }
  QString returnWindow() const { return m_return; }
  QVariantList items() const { return m_items; }
  QVariantList continueEntries() const;
  QVariantList applications() const;
  QVariantList webShortcuts() const { return m_webShortcuts; }
  QVariantList favorites() const;
  bool favoritesReady() const {
    return m_favoritesLoaded && !m_favoritesError && m_connected;
  }
  bool favoriteSaving() const { return m_favoriteSaving; }
  QString favoriteError() const { return m_favoriteError; }
  Q_INVOKABLE void setFavorite(const QString &id, bool pinned);
  Q_INVOKABLE void terminate(const QString &id);
  QString homeState() const;
  bool buildMismatch() const { return m_connected && m_buildMismatch; }
  // Exit with the status that asks the platform to restart Arinn (ADR-0098).
  Q_INVOKABLE void restartArinn();
  QString applicationsState() const;
  QStringList busyEntryIds() const { return m_pendingIds.values(); }
  QString message() const { return m_message; }
  QVariantMap shortcutEditor() const { return m_shortcutEditor; }
  bool shortcutSaving() const { return m_shortcutSaving; }
  QString shortcutError() const { return m_shortcutError; }
  Q_INVOKABLE void editShortcut(const QString &id = {});
  Q_INVOKABLE void closeShortcutEditor();
  Q_INVOKABLE void saveShortcut(const QString &title, const QString &url);
  Q_INVOKABLE void removeShortcut();
  Q_INVOKABLE void back();
  Q_INVOKABLE void launch(const QString &id);
  Q_INVOKABLE void activate(const QString &id);
public slots:
  void refresh();
private slots:
  void reloadCatalog();
  void reloadShortcuts();
  void reloadFavorites();
signals:
  void changed();
  void itemsChanged();
  void applicationsChanged();
  void webShortcutsChanged();
  void continueEntriesChanged();
  void screenRequested(const QString &name);

private:
  void readFavorites();
  QVariantMap favoriteTarget(const QString &id) const;
  bool isFavorite(const QVariantMap &target) const;
  QVariantList m_favoriteTargets;
  quint64 m_favoriteRevision = 0, m_favoriteTicket = 0;
  bool m_favoritesLoaded = false, m_favoritesError = false,
       m_favoritesRequested = false, m_favoriteSaving = false, m_favoriteReconcile = false;
  QString m_favoriteError;
  void mutateShortcut(const char *method, const QVariantList &args);
  QHash<QString, QVariantMap> m_shortcutRecords;
  QVariantMap m_shortcutEditor;
  QString m_shortcutError;
  bool m_shortcutSaving = false;
  quint64 m_shortcutRevision = 0, m_editorRevision = 0, m_editorTicket = 0;
  QSet<QString> m_pendingIds;
  struct Catalog;
  void readProviders(const std::shared_ptr<Catalog> &catalog,
                     const QString &token = {});
  void readApplications(const std::shared_ptr<Catalog> &catalog, int index = 0,
                        const QString &token = {});
  void readShortcuts(quint64 epoch);
  void checkBuild(quint64 epoch);
  bool m_buildRequested = false, m_buildMismatch = false;
  void finishCatalog(const std::shared_ptr<Catalog> &catalog, bool success);
  void runApplication(const QString &id, const QVariantList &entries,
                      const char *method);
  void openShortcut(const QString &id);
  void beginOperation(const QString &id, const QDBusMessage &msg);
  void setResultMessage(const QString &text);
  void watchOperation(const QString &id, const QString &operationPath,
                      quint64 epoch);
  void clearShortcuts();
  void publishState();
  // Home load and Return timing for diagnostics (ADR-0093).
  void traceFlows();
  FlowTrace::Flow m_homeFlow, m_returnFlow;
  QVariantList m_publishedItems, m_publishedContinue, m_publishedApplications,
      m_publishedShortcuts;
  QVariantList m_applications, m_webShortcuts;
  bool m_connected = false, m_available = false, m_home = true,
       m_refreshing = false, m_dirty = false, m_loaded = false,
       m_snapshotError = false, m_shortcutsLoaded = false,
       m_shortcutsError = false, m_shortcutsRequested = false,
       m_forceShortcutReload = false;
  quint64 m_connectionEpoch = 0, m_snapshotRevision = 0, m_shortcutTicket = 0;
  QString m_generation, m_return, m_message, m_daemonEpoch, m_daemonOwner;
  QVariantList m_items;
  static constexpr int resultMessageMs = 3000;
  // A Return the daemon has not carried out by then is reported as Timeout.
  static constexpr qint64 returnTraceMs = 5000;
  QTimer m_timer;
  // Clears an action result such as "Done" after resultMessageMs.
  QTimer m_resultTimer;
  QString m_resultMessage;
};

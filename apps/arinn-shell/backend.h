/*
 * SPDX-FileCopyrightText: 2026 Arinn contributors
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#pragma once
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
  Q_PROPERTY(QString homeState READ homeState NOTIFY changed)
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
  QString homeState() const;
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
signals:
  void changed();
  void itemsChanged();
  void applicationsChanged();
  void webShortcutsChanged();
  void continueEntriesChanged();
  void screenRequested(const QString &name);

private:
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
  void finishCatalog(const std::shared_ptr<Catalog> &catalog, bool success);
  void runApplication(const QString &id, const QVariantList &entries,
                      const char *method);
  void openShortcut(const QString &id);
  void beginOperation(const QString &id, const QDBusMessage &msg);
  void watchOperation(const QString &id, const QString &operationPath,
                      quint64 epoch);
  void clearShortcuts();
  void publishState();
  QVariantList m_publishedItems, m_publishedContinue, m_publishedApplications,
      m_publishedShortcuts;
  QVariantList m_applications, m_webShortcuts;
  bool m_connected = false, m_available = false, m_home = true,
       m_refreshing = false, m_dirty = false, m_loaded = false,
       m_snapshotError = false, m_shortcutsLoaded = false,
       m_shortcutsError = false, m_shortcutsRequested = false,
       m_forceShortcutReload = false;
  quint64 m_connectionEpoch = 0, m_snapshotRevision = 0, m_shortcutTicket = 0;
  QString m_generation, m_return, m_message, m_daemonEpoch;
  QVariantList m_items;
  QTimer m_timer;
};

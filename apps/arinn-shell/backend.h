/*
 * SPDX-FileCopyrightText: 2026 Arinn contributors
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#pragma once
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
  Q_PROPERTY(QString returnWindow READ returnWindow NOTIFY changed)
  Q_PROPERTY(QVariantList items READ items NOTIFY itemsChanged)
  Q_PROPERTY(QVariantList continueEntries READ continueEntries NOTIFY
                 continueEntriesChanged)
  Q_PROPERTY(
      QVariantList applications READ applications NOTIFY applicationsChanged)
  Q_PROPERTY(QString homeState READ homeState NOTIFY changed)
  Q_PROPERTY(QStringList busyEntryIds READ busyEntryIds NOTIFY changed)
  Q_PROPERTY(QString message READ message NOTIFY changed)
public:
  explicit Backend(QObject *parent = nullptr);
  bool connected() const { return m_connected; }
  bool available() const { return m_available; }
  bool homeVisible() const { return m_home; }
  QString returnWindow() const { return m_return; }
  QVariantList items() const { return m_items; }
  QVariantList continueEntries() const;
  QVariantList applications() const { return m_applications; }
  QString homeState() const;
  QStringList busyEntryIds() const { return m_pendingIds.values(); }
  QString message() const { return m_message; }
  Q_INVOKABLE void back();
  Q_INVOKABLE void launch(const QString &id);
  Q_INVOKABLE void activate(const QString &id);
public slots:
  void refresh();
signals:
  void changed();
  void itemsChanged();
  void applicationsChanged();
  void continueEntriesChanged();
  void screenRequested(const QString &name);

private:
  QSet<QString> m_pendingIds;
  struct Catalog;
  void readProviders(const std::shared_ptr<Catalog> &catalog,
                     const QString &token = {});
  void readApplications(const std::shared_ptr<Catalog> &catalog, int index = 0,
                        const QString &token = {});
  void finishCatalog(const std::shared_ptr<Catalog> &catalog, bool success);
  void runApplication(const QString &id, const QVariantList &entries,
                      const char *method);
  void publishState();
  QVariantList m_publishedItems, m_publishedContinue, m_publishedApplications;
  QVariantList m_applications;
  bool m_connected = false, m_available = false, m_home = true,
       m_refreshing = false, m_dirty = false, m_loaded = false,
       m_snapshotError = false;
  quint64 m_connectionEpoch = 0, m_snapshotRevision = 0;
  QString m_generation, m_return, m_message;
  QVariantList m_items;
  QTimer m_timer;
};

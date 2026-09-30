/*
 * SPDX-FileCopyrightText: 2026 Arinn contributors
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#pragma once
#include <QObject>
#include <QTimer>
#include <QVariantList>
#include <memory>
#include <QtGlobal>

// Reads org.arinn.Library1 for the shell. Screens only see presentation
// properties. Launch is an operation; the shell never sees a file URI.
class LibraryCatalog : public QObject {
  Q_OBJECT
  Q_PROPERTY(QString libraryState READ libraryState NOTIFY catalogChanged)
  Q_PROPERTY(QVariantList tabs READ tabs NOTIFY catalogChanged)
  Q_PROPERTY(QVariantList entries READ entries NOTIFY catalogChanged)
  Q_PROPERTY(QString operationState READ operationState NOTIFY operationChanged)
  Q_PROPERTY(QString launchingId READ launchingId NOTIFY operationChanged)
public:
  explicit LibraryCatalog(QObject *parent = nullptr);
  QString libraryState() const;
  QVariantList tabs() const { return m_tabs; }
  QVariantList entries() const { return m_entries; }
  QString operationState() const { return m_operation; }
  QString launchingId() const { return m_launchingId; }
  Q_INVOKABLE void launch(const QString &id);
public slots:
  void refresh();
signals:
  void catalogChanged();
  void operationChanged();
  void sessionLaunched(const QString &presentationId);

private:
  struct ItemRow {
    QString id;
    QString localId;
    QString providerId;
    QString path;
    QString generation;
    QString tabId;
    QString collectionLocal;
    QString parentLocal;
    QString title;
    QString kind;
    bool hasYear = false;
    int year = 0;
    bool hasSeason = false;
    quint32 season = 0;
    bool hasEpisode = false;
    quint32 episode = 0;
    qint64 durationUs = -1;
    QString poster;
    QString fanart;
    bool available = false;
    bool hasResume = false;
    qint64 resumePosition = 0;
    qint64 resumeDuration = -1;
    bool resumeCompleted = false;
    quint64 resumeObserved = 0;
    QString backend;
  };
  struct CollectionRow {
    QString id;
    QString localId;
    QString providerId;
    QString title;
    QString kind;
  };
  struct ResumeRow {
    QString localId;
    qint64 position = 0;
    qint64 duration = -1;
    bool completed = false;
    quint64 observed = 0;
  };
  struct Catalog;
  void readProviders(const std::shared_ptr<Catalog> &catalog,
                     const QString &token = {});
  void readLibrary(const std::shared_ptr<Catalog> &catalog, int index,
                   const QString &token = {});
  void readFeatures(const std::shared_ptr<Catalog> &catalog, int index);
  void readResumes(const std::shared_ptr<Catalog> &catalog, int index,
                   const QString &token = {});
  void failProvider(const std::shared_ptr<Catalog> &catalog, int index);
  void dropResumes(const std::shared_ptr<Catalog> &catalog, int index);
  void nextProvider(const std::shared_ptr<Catalog> &catalog, int index);
  bool acceptProvider(const std::shared_ptr<Catalog> &catalog, int index);
  void applyResumes(const std::shared_ptr<Catalog> &catalog, int index);
  void finishCatalog(const std::shared_ptr<Catalog> &catalog, bool success);
  void publish();
  void setOperation(const QString &state);
  void watchOperation(const QString &objectPath, quint64 ticket, quint64 epoch);
  const ItemRow *findId(const QString &id) const;
  const ItemRow *seriesTarget(const ItemRow &series) const;
  const ItemRow *launchTarget(const ItemRow &row) const;
  QVariantList m_tabs;
  QVariantList m_entries;
  QList<ItemRow> m_items;
  QList<CollectionRow> m_collections;
  QString m_operation;
  QString m_launchingId;
  bool m_connected = false, m_loaded = false, m_error = false,
       m_unreachable = false, m_refreshing = false, m_dirty = false;
  quint64 m_epoch = 0, m_commandTicket = 0;
  QTimer m_timer;
};

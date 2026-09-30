/*
 * SPDX-FileCopyrightText: 2026 Arinn contributors
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#pragma once
#include <QObject>
#include <QStringList>
#include <QTimer>
#include <QVariantList>
#include <memory>
#include <QtGlobal>

// Reads org.arinn.Media1 for the shell. The screens only see presentation
// properties. Playback commands go back through this object as operations.
class MediaCatalog : public QObject {
  Q_OBJECT
  Q_PROPERTY(QVariantList continueEntries READ continueEntries NOTIFY
                 continueEntriesChanged)
  Q_PROPERTY(QString sessionState READ sessionState NOTIFY sessionChanged)
  Q_PROPERTY(QString sessionTitle READ sessionTitle NOTIFY sessionChanged)
  Q_PROPERTY(QString sessionSubtitle READ sessionSubtitle NOTIFY sessionChanged)
  Q_PROPERTY(QString playbackState READ playbackState NOTIFY sessionChanged)
  Q_PROPERTY(double durationUs READ durationUs NOTIFY sessionChanged)
  Q_PROPERTY(double positionUs READ positionUs NOTIFY positionChanged)
  Q_PROPERTY(QStringList sessionActions READ sessionActions NOTIFY sessionChanged)
  Q_PROPERTY(QString operationState READ operationState NOTIFY operationChanged)
  Q_PROPERTY(QString selectedId READ selectedId NOTIFY sessionChanged)
  Q_PROPERTY(QString sessionArtwork READ sessionArtwork NOTIFY sessionChanged)
public:
  explicit MediaCatalog(QObject *parent = nullptr);
  QVariantList continueEntries() const { return m_continue; }
  QString sessionState() const;
  QString sessionTitle() const;
  QString sessionSubtitle() const;
  QString playbackState() const;
  double durationUs() const;
  double positionUs() const;
  QStringList sessionActions() const;
  QString operationState() const { return m_operation; }
  QString selectedId() const;
  QString sessionArtwork() const;
  Q_INVOKABLE void select(const QString &id);
  Q_INVOKABLE void play();
  Q_INVOKABLE void pause();
  Q_INVOKABLE void playPause();
  Q_INVOKABLE void seek(double targetUs);
  Q_INVOKABLE void next();
  Q_INVOKABLE void previous();
  Q_INVOKABLE void stop();
public slots:
  void refresh();
signals:
  void continueEntriesChanged();
  void sessionChanged();
  void positionChanged();
  void operationChanged();

private:
  struct SessionRow {
    QString id;
    QString providerId;
    QString path;
    QString generation;
    QString localId;
    QString title;
    QString subtitle;
    QString artworkId;
    QString state;
    qint64 durationUs = -1;
    qint64 positionUs = 0;
    quint64 anchorUs = 0;
    QStringList actions;
    QStringList wireActions;
  };
  struct Catalog;
  void readProviders(const std::shared_ptr<Catalog> &catalog,
                     const QString &token = {});
  void readSessions(const std::shared_ptr<Catalog> &catalog, int index,
                    const QString &token = {});
  void failProvider(const std::shared_ptr<Catalog> &catalog, int index);
  void finishCatalog(const std::shared_ptr<Catalog> &catalog, bool success);
  void publish();
  void setOperation(const QString &state);
  void command(const QString &action, double seekTarget = 0, bool seeking = false);
  void watchOperation(const QString &objectPath, quint64 ticket, quint64 epoch);
  void syncClock();
  const SessionRow *selected() const;
  const SessionRow *findId(const QString &id) const;
  static qint64 displayedPosition(const SessionRow &row);
  QVariantList m_continue;
  QList<SessionRow> m_sessions;
  QString m_explicitId;
  QString m_operation;
  bool m_connected = false, m_loaded = false, m_snapshotError = false,
       m_refreshing = false, m_dirty = false;
  quint64 m_epoch = 0, m_commandTicket = 0;
  QTimer m_timer;
  QTimer m_clock;
};

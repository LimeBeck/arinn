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
#include <QDBusMessage>
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
  Q_PROPERTY(bool playerSession READ playerSession NOTIFY sessionChanged)
  Q_PROPERTY(bool tracksAvailable READ tracksAvailable NOTIFY sessionChanged)
  Q_PROPERTY(bool timelineAvailable READ timelineAvailable NOTIFY sessionChanged)
  Q_PROPERTY(QVariantList audioTracks READ audioTracks NOTIFY tracksChanged)
  Q_PROPERTY(QVariantList subtitleTracks READ subtitleTracks NOTIFY tracksChanged)
  Q_PROPERTY(QString audioTrackId READ audioTrackId NOTIFY tracksChanged)
  Q_PROPERTY(QString subtitleTrackId READ subtitleTrackId NOTIFY tracksChanged)
  Q_PROPERTY(QVariantList chapters READ chapters NOTIFY timelineChanged)
  Q_PROPERTY(double bufferedSeconds READ bufferedSeconds NOTIFY positionChanged)
  Q_PROPERTY(QString thumbnail READ thumbnail NOTIFY thumbnailChanged)
  Q_PROPERTY(bool thumbnailWanted READ thumbnailWanted WRITE setThumbnailWanted
                 NOTIFY thumbnailWantedChanged)
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
  bool playerSession() const;
  bool tracksAvailable() const;
  bool timelineAvailable() const;
  QVariantList audioTracks() const;
  QVariantList subtitleTracks() const;
  QString audioTrackId() const;
  QString subtitleTrackId() const;
  QVariantList chapters() const { return m_chapters; }
  double bufferedSeconds() const;
  QString thumbnail() const { return m_thumbnail; }
  bool thumbnailWanted() const { return m_thumbnailWanted; }
  void setThumbnailWanted(bool wanted);
  Q_INVOKABLE void select(const QString &id);
  Q_INVOKABLE void play();
  Q_INVOKABLE void pause();
  Q_INVOKABLE void playPause();
  Q_INVOKABLE void seek(double targetUs);
  Q_INVOKABLE void next();
  Q_INVOKABLE void previous();
  Q_INVOKABLE void stop();
  Q_INVOKABLE void selectAudio(const QString &trackId);
  Q_INVOKABLE void selectSubtitle(const QString &trackId);
public slots:
  void refresh();
signals:
  void continueEntriesChanged();
  void sessionChanged();
  void positionChanged();
  void operationChanged();
  void tracksChanged();
  void timelineChanged();
  void thumbnailChanged();
  void thumbnailWantedChanged();

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
    QStringList features;
    quint64 revision = 0;
  };
  struct TrackRow {
    QString id;
    QString kind;
    QString language;
    QString label;
    QString codec;
    quint32 channels = 0;
    bool selected = false;
  };
  struct RangeRow {
    qint64 startUs = 0;
    qint64 endUs = 0;
  };
  struct Catalog;
  void readProviders(const std::shared_ptr<Catalog> &catalog,
                     const QString &token = {});
  void readFeatures(const std::shared_ptr<Catalog> &catalog, int index);
  void readSessions(const std::shared_ptr<Catalog> &catalog, int index,
                    const QString &token = {});
  void failProvider(const std::shared_ptr<Catalog> &catalog, int index);
  void finishCatalog(const std::shared_ptr<Catalog> &catalog, bool success);
  void publish();
  void setOperation(const QString &state);
  void command(const QString &action, double seekTarget = 0, bool seeking = false);
  void extensionCommand(const char *method, const QStringList &middle);
  void dispatch(const QDBusMessage &msg, quint64 ticket, quint64 epoch);
  void watchOperation(const QString &objectPath, quint64 ticket, quint64 epoch);
  void loadExtensions();
  void finishExtensionLoad();
  void readTracks(quint64 epoch, const SessionRow &row);
  void readTimeline(quint64 epoch, const SessionRow &row);
  void applyTracks(const QList<TrackRow> &tracks);
  void clearExtensions();
  void requestDisplayedThumbnail();
  void requestThumbnailAt(qint64 positionUs);
  void sendThumbnail();
  void watchThumbnail(const QString &objectPath, quint64 ticket, quint64 epoch,
                      qint64 positionUs);
  bool featureOn(const QString &flag) const;
  QVariantMap trackMap(const TrackRow &track) const;
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
  QTimer m_thumbnailTimer;
  QList<TrackRow> m_tracks;
  QList<RangeRow> m_ranges;
  QVariantList m_chapters;
  QString m_subtitleOffId;
  QString m_thumbnail;
  QString m_boundSessionId;
  quint64 m_loadedRevision = 0;
  quint64 m_loadingRevision = 0;
  quint64 m_extensionEpoch = 0;
  quint64 m_thumbnailTicket = 0;
  qint64 m_thumbnailPosition = -1;
  qint64 m_pendingThumb = -1;
  qint64 m_thumbnailInFlightAt = -1;
  bool m_extensionLoading = false;
  bool m_extensionsLoaded = false;
  bool m_tracksDone = true;
  bool m_timelineDone = true;
  bool m_thumbnailWanted = false;
  bool m_thumbFromSeek = false;
  bool m_thumbnailInFlight = false;
};

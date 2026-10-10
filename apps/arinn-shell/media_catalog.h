/*
 * SPDX-FileCopyrightText: 2026 Arinn contributors
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#pragma once
#include "flow_trace.h"
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
  Q_PROPERTY(
      QStringList sessionActions READ sessionActions NOTIFY sessionChanged)
  Q_PROPERTY(QString operationState READ operationState NOTIFY operationChanged)
  Q_PROPERTY(QString selectedId READ selectedId NOTIFY sessionChanged)
  Q_PROPERTY(QString sessionArtwork READ sessionArtwork NOTIFY sessionChanged)
  Q_PROPERTY(bool windowAvailable READ windowAvailable NOTIFY sessionChanged)
  Q_PROPERTY(QString presentationState READ presentationState NOTIFY presentationChanged)
  Q_PROPERTY(bool inputLayerActive READ inputLayerActive NOTIFY presentationChanged)
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
  bool windowAvailable() const;
  QString presentationState() const { return m_presentation; }
  bool inputLayerActive() const { return m_presentation == QStringLiteral("pending") || m_presentation == QStringLiteral("active"); }
  Q_INVOKABLE void present();
  Q_INVOKABLE void release();
  Q_INVOKABLE void abandonPresentation();
  Q_INVOKABLE void requestHome();
  // One specification §14.1 JSON line on stderr when playback does not get the
  // player (#369). Written directly, not through Qt logging, so the line stays
  // parseable JSON. Fixed names only: never a title, session id or window.
  Q_INVOKABLE void logSurface(const QString &surface, const QString &reason) const;
  static QByteArray surfaceRecord(const QString &surface, const QString &reason,
                                  const QString &presentation, const QString &session,
                                  bool inputLayer, bool windowAvailable);
  // For a stopped session that plays a library item: {provider, local} of
  // that item, so Continue resumes it through the library (#247). Empty
  // otherwise: a stopped managed session cannot be presented again.
  Q_INVOKABLE QVariantMap resumeTarget(const QString &id) const;
  // {provider, local} of the library item any session plays, or an empty
  // map. Home uses it to show that item's backdrop (#365).
  Q_INVOKABLE QVariantMap libraryContent(const QString &id) const;
  // Play/pause one session without selecting it: a Continue card linked to
  // a browser tab sends this to its media session (ADR-0096).
  // `play`, `pause` or `playPause` for the session `id`, without selecting
  // it (a Home card linked to it, ADR-0096).
  Q_INVOKABLE void sessionAction(const QString &id, const QString &action);
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
  void linksChanged(const QString &epoch, qulonglong revision);
  void windowChanged(const QString &generation);
  void homeRequested(const QString &epoch, const QString &target, const QString &output);
signals:
  void presentationChanged();
  void presentationHomeRequested();
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
    // The library item this session plays, when it came from a library.
    QString contentProvider;
    QString contentLocal;
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
  void refreshStatus(const QString &owner, quint64 epoch);
  void readProviders(const std::shared_ptr<Catalog> &catalog,
                     const QString &token = {});
  void readFeatures(const std::shared_ptr<Catalog> &catalog, int index);
  void readSessions(const std::shared_ptr<Catalog> &catalog, int index,
                    const QString &token = {});
  void failProvider(const std::shared_ptr<Catalog> &catalog, int index);
  void finishCatalog(const std::shared_ptr<Catalog> &catalog, bool success);
  void publish();
  void setPresentation(const QString &state);
  void failPresentation(const QString &state);
  void failRelease();
  void checkPresentedWindow(quint64 ticket, quint64 epoch, quint64 deadline);
  void resolveWindow(SessionRow row, quint64 ticket, quint64 epoch, quint64 deadline);
  void windowCommand(const SessionRow &row, bool presenting, quint64 ticket, quint64 epoch, bool detachedRelease = false);
  void watchWindowOperation(const QString &objectPath, bool presenting, quint64 ticket, quint64 epoch, quint64 deadline);
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
  void requestThumbnailAt(qint64 positionUs, bool restart);
  void sendThumbnail();
  void watchThumbnail(const QString &objectPath, quint64 ticket, quint64 epoch,
                      qint64 positionUs);
  bool featureOn(const QString &flag) const;
  QVariantMap trackMap(const TrackRow &track) const;
  void syncClock();
  const SessionRow *selected() const;
  const SessionRow *findId(const QString &id) const;
  static qint64 displayedPosition(const SessionRow &row);
  // MediaAssociation1 links (ADR-0096): a session and the presentation ID
  // of the browser tab that plays it.
  struct LinkRow {
    QString provider;
    QString generation;
    QString session;
    QString tab;
    bool operator==(const LinkRow &) const = default;
  };
  QList<LinkRow> m_links;
  // The revision of the last applied snapshot of this daemon epoch; an
  // older reply is discarded (media-association contract).
  quint64 m_linksRevision = 0;
  // The revision a `Changed` announced; a reply below it is not applied.
  quint64 m_linksWanted = 0;
  void readLinks(const QString &owner, quint64 epoch);
  QVariantList m_continue;
  QList<SessionRow> m_sessions;
  QString m_explicitId;
  QString m_operation;
  QString m_presentation;
  QString m_daemonEpoch;
  QString m_daemonOwner;
  QString m_windowGeneration, m_windowToken;
  bool m_windowCheckInFlight = false;
  // Presentation timing for diagnostics (ADR-0093): present() until the
  // window check confirms the input layer, or the failure.
  FlowTrace::Flow m_presentFlow;
  SessionRow m_presentRow;
  SessionRow m_releaseRow;
  quint64 m_releaseEpoch = 0;
  quint64 m_presentationTicket = 0;
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

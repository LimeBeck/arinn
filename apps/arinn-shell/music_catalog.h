/* SPDX-FileCopyrightText: 2026 Arinn contributors
 * SPDX-License-Identifier: GPL-3.0-or-later */
#pragma once
#include <QDeadlineTimer>
#include <QMap>
#include <QPair>
#include <QObject>
#include <QSet>
#include <QTimer>
#include <QVariantList>
#include <QVariantMap>
#include <optional>
class MusicCatalog : public QObject {
  Q_OBJECT
  Q_PROPERTY(QVariantList sources READ sources NOTIFY sourcesChanged)
  Q_PROPERTY(QVariantList entries READ entries NOTIFY entriesChanged)
  Q_PROPERTY(QVariantMap state READ state NOTIFY stateChanged)
  Q_PROPERTY(QVariantMap activeState READ activeState NOTIFY activeChanged)
  Q_PROPERTY(
      QString activeSourceName READ activeSourceName NOTIFY activeChanged)
  Q_PROPERTY(QString sourceId READ sourceId NOTIFY sourcesChanged)
  // The provider of the active session, which may differ from the browsed
  // source: the Music root marks the entry that plays (#800).
  Q_PROPERTY(QString activeSourceId READ activeSourceId NOTIFY activeChanged)
  Q_PROPERTY(QString errorText READ errorText NOTIFY statusChanged)
  Q_PROPERTY(bool busy READ busy NOTIFY statusChanged)
  Q_PROPERTY(bool transportBusy READ transportBusy NOTIFY statusChanged)
  Q_PROPERTY(bool more READ more NOTIFY statusChanged)
  Q_PROPERTY(QVariantList homeEntries READ homeEntries NOTIFY activeChanged)
  // Sources turned off in the Sources sheet (#842): a session of one of
  // them has no presence on Home or in the Quick menu.
  Q_PROPERTY(QStringList disabledSources READ disabledSources WRITE
                 setDisabledSources NOTIFY activeChanged)
  // Session reads (GetActive) answered so far: a caller waiting for the
  // state after a command counts these, not activeChanged (#842).
  Q_PROPERTY(int activeReads READ activeReads NOTIFY activeReadsChanged)
  // The last session read answered: until then, or after one failed, the
  // active session is unknown (#842).
  Q_PROPERTY(bool activeKnown READ activeKnown NOTIFY activeReadsChanged)
  Q_PROPERTY(QString sectionTitle READ sectionTitle CONSTANT)
  // The active session as the Quick menu's Now playing row reads it
  // ({id, title, subtitle, state, playPause}), or an empty map.
  Q_PROPERTY(QVariantMap nowPlaying READ nowPlaying NOTIFY problemsChanged)
  // "track · artist" for the status area while the session plays; empty on
  // pause and without a session (brief *Presence*, MH-4, #803).
  Q_PROPERTY(QString statusLine READ statusLine NOTIFY problemsChanged)
  // [provider ID, station ID] pairs of stored stations the provider answered
  // `radio` with NotFound or Unavailable for (music.md). Changes when one is
  // found missing, starts again, or its provider restarts or leaves.
  Q_PROPERTY(QVariantList unresolvedStations READ unresolvedStations NOTIFY
                 unresolvedChanged)
  // The active session plays or is loading its next track: interface sounds
  // stay quiet (spec §5.6, #756 M26).
  Q_PROPERTY(bool audible READ audible NOTIFY activeChanged)
  // Why the active session stopped, while it stays stopped; empty while it
  // plays, is paused or loads again (#756 M5, M8).
  Q_PROPERTY(
      QString playbackProblem READ playbackProblem NOTIFY problemsChanged)
  // The one status line under the music screens: a failed request, a
  // denied account, a degraded source or a stopped session, with a reason.
  Q_PROPERTY(QString notice READ notice NOTIFY problemsChanged)
public:
  explicit MusicCatalog(QObject *parent = nullptr);
  QVariantList sources() const { return m_sources; }
  QVariantList entries() const { return m_entries; }
  QVariantMap state() const { return m_state; }
  QVariantMap activeState() const { return m_active; }
  QString activeSourceName() const;
  QString sourceId() const { return m_sourceId; }
  QString activeSourceId() const { return m_activeId; }
  QString errorText() const { return m_error; }
  bool busy() const {
    return m_commandBusy || m_pageBusy ||
           m_state.value("catalogue_loading").toBool();
  }
  bool transportBusy() const { return m_commandBusy; }
  bool more() const { return !m_next.isEmpty(); }
  QString sectionTitle() const;
  QVariantList homeEntries() const;
  QVariantMap nowPlaying() const;
  QStringList disabledSources() const { return m_disabledSources; }
  int activeReads() const { return m_activeReads; }
  bool activeKnown() const { return m_activeKnown; }
  void setDisabledSources(const QStringList &sources);
  QString browsableSource() const;
  QString statusLine() const;
  QVariantList unresolvedStations() const;
  bool audible() const;
  QString playbackProblem() const;
  QString notice() const;
  // The id of the active session in Home's Continue row and the Quick menu.
  static QString sessionEntryId();
  // Play/Pause from Home's card and the Quick menu.
  Q_INVOKABLE void togglePlayback();
  // A pinned station the provider answered `radio` with NotFound for: Home
  // marks its pin unavailable and never starts another station (#804).
  Q_INVOKABLE bool stationResolved(const QString &providerId,
                                   const QString &stationId) const;
  // The image URL of a provider's opaque artwork ID through the source's
  // current endpoint and generation; empty while the source is absent
  // (pinned stations on Home, #804).
  Q_INVOKABLE QString artworkUrl(const QString &providerId,
                                 const QString &artworkId) const;
  // Mirrors Arinn's pin to the provider's `pin` action where it covers the
  // station that plays now (music brief, Favourites).
  Q_INVOKABLE void syncProviderPin(const QString &providerId,
                                   const QString &stationId, bool pinned);
  Q_INVOKABLE void choose(const QString &id);
  Q_INVOKABLE void browse(const QString &facet,
                          const QString &parent = QString(),
                          const QString &search = QString());
  Q_INVOKABLE void loadMore();
  Q_INVOKABLE void command(const QString &action,
                           const QString &entity = QString(),
                           const QString &occurrence = QString(),
                           qint64 positionUs = 0,
                           const QString &text = QString());
  Q_INVOKABLE void transport(const QString &action,
                             const QString &entity = QString(),
                             const QString &occurrence = QString(),
                             qint64 positionUs = 0,
                             const QString &text = QString());
  Q_INVOKABLE void refresh();
signals:
  void activeReadsChanged();
  void sourcesChanged();
  void entriesChanged();
  void stateChanged();
  void statusChanged();
  void activeChanged();
  // The browse position (album, artist or playlist) no longer applies: the
  // source or its generation changed.
  void browseReset();
  void problemsChanged();
  void unresolvedChanged();
  // A page read failed; the list it was for does not arrive.
  void pageFailed();

private:
  void providers(const QString &token, quint64 ticket, const QString &snapshot,
                 QVariantList rows, QSet<QString> tokens);
  QString statusText(const QString &status) const;
  void forgetStaleUnresolved();
  void mirrorPin();
  // The provider pin still to mirror (#804).
  struct PinMirror {
    QString provider, station;
    bool pinned = false;
  };
  std::optional<PinMirror> m_pinMirror;
  // Commands that ended, and how many of them the last active read had seen
  // end before it was asked.
  quint64 m_commandsDone = 0, m_activeSeen = 0;
  // Until then, after a command that timed out, active reads do not count.
  QDeadlineTimer m_activeFenceAfter{0};
  // (provider ID, station ID) -> the provider generation that answered. A
  // pair, not a joined string: protocol IDs may hold any character but NUL.
  QMap<QPair<QString, QString>, QString> m_unresolved;
  void updateProblems();
  // The derived values last announced through problemsChanged.
  QVariantMap m_nowPlaying;
  QStringList m_disabledSources;
  int m_activeReads = 0;
  bool m_activeKnown = false;
  QString m_playbackProblem, m_notice;
  void readState();
  void readActive();
  void readPage(bool append);
  void fail(const QString &code);
  void send(const QString &action, const QString &entity,
            const QString &occurrence, qint64 positionUs, bool active,
            const QString &text = QString());
  QVariantList m_sources, m_entries;
  QVariantMap m_state;
  QVariantMap m_active;
  QString m_activeId, m_activeGeneration;
  QString m_sourceId, m_path, m_generation, m_error,
      m_facet = QStringLiteral("albums"), m_parent, m_search, m_next;
  quint64 m_commandTicket = 0, m_epoch = 0, m_registryTicket = 0,
          m_pageTicket = 0, m_stateTicket = 0, m_catalogueRevision = 0,
          m_pageRevision = 0;
  bool m_commandBusy = false, m_pageBusy = false, m_registryBusy = false,
       m_stateBusy = false;
  bool m_activeBusy = false, m_commandWaitingRefresh = false,
       m_stateError = false;
  QSet<QString> m_pageTokens;
  bool m_uncertainStart = false;
  QString m_startProvider, m_startGeneration, m_startSession;
  QTimer m_timer;
};

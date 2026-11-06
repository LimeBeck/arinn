/* SPDX-FileCopyrightText: 2026 Arinn contributors
 * SPDX-License-Identifier: GPL-3.0-or-later */
#pragma once
#include <QObject>
#include <QTimer>
#include <QVariantList>
#include <QVariantMap>
#include <QSet>
class MusicCatalog : public QObject {
  Q_OBJECT
  Q_PROPERTY(QVariantList sources READ sources NOTIFY sourcesChanged)
  Q_PROPERTY(QVariantList entries READ entries NOTIFY entriesChanged)
  Q_PROPERTY(QVariantMap state READ state NOTIFY stateChanged)
  Q_PROPERTY(QVariantMap activeState READ activeState NOTIFY activeChanged)
  Q_PROPERTY(QString activeSourceName READ activeSourceName NOTIFY activeChanged)
  Q_PROPERTY(QString sourceId READ sourceId NOTIFY sourcesChanged)
  Q_PROPERTY(QString errorText READ errorText NOTIFY statusChanged)
  Q_PROPERTY(bool busy READ busy NOTIFY statusChanged)
  Q_PROPERTY(bool transportBusy READ transportBusy NOTIFY statusChanged)
  Q_PROPERTY(bool more READ more NOTIFY statusChanged)
  Q_PROPERTY(QVariantList homeEntries READ homeEntries NOTIFY activeChanged)
  Q_PROPERTY(QString sectionTitle READ sectionTitle CONSTANT)
  // The active session as the Quick menu's Now playing row reads it
  // ({id, title, subtitle, state, playPause}), or an empty map.
  Q_PROPERTY(QVariantMap nowPlaying READ nowPlaying NOTIFY problemsChanged)
  // The active session plays or is loading its next track: interface sounds
  // stay quiet (spec §5.6, #756 M26).
  Q_PROPERTY(bool audible READ audible NOTIFY activeChanged)
  // Why the active session stopped, while it stays stopped; empty while it
  // plays, is paused or loads again (#756 M5, M8).
  Q_PROPERTY(QString playbackProblem READ playbackProblem NOTIFY problemsChanged)
  // The one status line under the music screens: a failed request, a
  // denied account, a degraded source or a stopped session, with a reason.
  Q_PROPERTY(QString notice READ notice NOTIFY problemsChanged)
public:
  explicit MusicCatalog(QObject *parent=nullptr);
  QVariantList sources() const{return m_sources;}
  QVariantList entries() const{return m_entries;}
  QVariantMap state() const{return m_state;}
  QVariantMap activeState() const{return m_active;}
  QString activeSourceName() const;
  QString sourceId() const{return m_sourceId;}
  QString errorText() const{return m_error;}
  bool busy() const{return m_commandBusy||m_pageBusy||m_state.value("catalogue_loading").toBool();}
  bool transportBusy() const{return m_commandBusy;}
  bool more() const{return !m_next.isEmpty();}
  QString sectionTitle() const;
  QVariantList homeEntries() const;
  QVariantMap nowPlaying() const;
  bool audible() const;
  QString playbackProblem() const;
  QString notice() const;
  // The id of the active session in Home's Continue row and the Quick menu.
  static QString sessionEntryId();
  // Play/Pause from Home's card and the Quick menu.
  Q_INVOKABLE void togglePlayback();
  Q_INVOKABLE void choose(const QString &id);
  Q_INVOKABLE void browse(const QString &facet,const QString &parent=QString(),const QString &search=QString());
  Q_INVOKABLE void loadMore();
  Q_INVOKABLE void command(const QString &action,const QString &entity=QString(),const QString &occurrence=QString(),qint64 positionUs=0,const QString &text=QString());
  Q_INVOKABLE void transport(const QString &action,const QString &entity=QString(),const QString &occurrence=QString(),qint64 positionUs=0,const QString &text=QString());
  Q_INVOKABLE void refresh();
signals:
  void sourcesChanged();void entriesChanged();void stateChanged();void statusChanged();void activeChanged();
  // The browse position (album, artist or playlist) no longer applies: the
  // source or its generation changed.
  void browseReset();
  void problemsChanged();
private:
  void providers(const QString &token,quint64 ticket,const QString &snapshot,QVariantList rows,QSet<QString> tokens);
  QString statusText(const QString &status) const;
  void readState();void readActive();void readPage(bool append);void fail(const QString &code);
  void send(const QString &action,const QString &entity,const QString &occurrence,qint64 positionUs,bool active,const QString &text=QString());
  QVariantList m_sources,m_entries;
  QVariantMap m_state;
  QVariantMap m_active;
  QString m_activeId,m_activeGeneration;
  QString m_sourceId,m_path,m_generation,m_error,m_facet=QStringLiteral("albums"),m_parent,m_search,m_next;
  quint64 m_commandTicket=0,m_epoch=0,m_registryTicket=0,m_pageTicket=0,m_stateTicket=0,m_catalogueRevision=0,m_pageRevision=0;
  bool m_commandBusy=false,m_pageBusy=false,m_registryBusy=false,m_stateBusy=false;
  bool m_activeBusy=false,m_commandWaitingRefresh=false,m_stateError=false;
  QSet<QString> m_pageTokens;
  bool m_uncertainStart=false;
  QString m_startProvider,m_startGeneration,m_startSession;
  QTimer m_timer;
};

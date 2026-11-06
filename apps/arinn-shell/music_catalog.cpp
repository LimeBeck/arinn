// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "music_catalog.h"
#include "asset_images.h"
#include <QtDBus>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>
#include <QQmlEngine>
#include <QUuid>
namespace {
constexpr auto service="org.arinn.Daemon1";
QDBusMessage call(const QString &path,const char *interface,const char *method){return QDBusMessage::createMethodCall(service,path,interface,method);}
QJsonObject object(const QDBusMessage &reply){
  if(reply.type()==QDBusMessage::ErrorMessage||reply.signature()!=QStringLiteral("s")||reply.arguments().size()!=1)return {};
  const auto bytes=reply.arguments().first().toString().toUtf8();if(bytes.size()>256*1024)return {};
  const auto doc=QJsonDocument::fromJson(bytes);return doc.isObject()?doc.object():QJsonObject();
}
QString code(const QDBusMessage &reply){auto value=reply.errorName().section('.',-1);return value.isEmpty()?QStringLiteral("Unavailable"):value;}
QVariantMap imageEntity(QVariantMap entity,const QString &id,const QString &path,const QString &generation) {
  entity.insert(QStringLiteral("image"),AssetImages::imageUrl(id,path,generation,entity.value(QStringLiteral("artwork")).toString()));return entity;
}
QVariantMap imageState(QVariantMap state,const QString &id,const QString &path,const QString &generation) {
  auto entry=[&](QVariantMap value){value.insert(QStringLiteral("track"),imageEntity(value.value(QStringLiteral("track")).toMap(),id,path,generation));return value;};
  if(!state.value(QStringLiteral("current")).toMap().isEmpty())state.insert(QStringLiteral("current"),entry(state.value(QStringLiteral("current")).toMap()));
  for(const auto *key:{"history","upcoming"}) {QVariantList values;for(const auto &value:state.value(key).toList())values.append(entry(value.toMap()));state.insert(key,values);}
  return state;
}
void registerMusic(){qmlRegisterType<MusicCatalog>("Arinn.Shell",1,0,"MusicCatalog");}
Q_COREAPP_STARTUP_FUNCTION(registerMusic)
}
MusicCatalog::MusicCatalog(QObject *parent):QObject(parent){
  // The derived lines read the status, the browsed state and the session.
  for(auto changed:{&MusicCatalog::statusChanged,&MusicCatalog::stateChanged,&MusicCatalog::activeChanged})
    connect(this,changed,this,&MusicCatalog::problemsChanged);
  auto *watch=new QDBusServiceWatcher(service,QDBusConnection::sessionBus(),QDBusServiceWatcher::WatchForOwnerChange,this);
  connect(watch,&QDBusServiceWatcher::serviceOwnerChanged,this,[this]{++m_commandTicket;++m_epoch;++m_pageTicket;++m_stateTicket;++m_registryTicket;m_registryBusy=m_pageBusy=m_stateBusy=m_commandBusy=m_activeBusy=m_commandWaitingRefresh=false;m_uncertainStart=false;m_path.clear();m_generation.clear();m_sources.clear();m_entries.clear();m_state.clear();m_active.clear();m_activeId.clear();m_activeGeneration.clear();m_next.clear();emit activeChanged();emit sourcesChanged();emit entriesChanged();emit stateChanged();emit statusChanged();refresh();});
  m_timer.setInterval(1000);connect(&m_timer,&QTimer::timeout,this,&MusicCatalog::refresh);m_timer.start();QTimer::singleShot(0,this,&MusicCatalog::refresh);
}
QString MusicCatalog::sectionTitle()const{return tr("Music");}
QVariantList MusicCatalog::homeEntries()const {
  const auto current=m_active.value("current").toMap();if(current.isEmpty())return {};
  const auto track=current.value("track").toMap();const auto playback=m_active.value("playback").toString();
  const bool playing=playback==QStringLiteral("playing"),paused=playback==QStringLiteral("paused");
  const auto status=playing?tr("Playing"):paused?tr("Paused"):playback==QStringLiteral("loading")?tr("Loading…"):tr("Stopped");
  return {QVariantMap{{"id",sessionEntryId()},{"kind",QStringLiteral("music")},{"title",track.value("title")},{"subtitle",track.value("artist")},{"artwork",track.value("image")},{"statusText",activeSourceName().isEmpty()?tr("Music · %1").arg(status):tr("%1 · %2").arg(activeSourceName(),status)},{"secondaryIcon",playing?QStringLiteral("pause"):QStringLiteral("play")},{"secondaryText",playing?tr("Pause"):tr("Play")},{"secondaryEnabled",playing||paused},{"busy",m_commandBusy}}};
}
void MusicCatalog::fail(const QString &value){
  if(value==QStringLiteral("Conflict"))m_error=tr("The music source changed. Try again.");
  else if(value==QStringLiteral("Timeout"))m_error=tr("The music source did not respond.");
  else if(value==QStringLiteral("Unsupported"))m_error=tr("This action is not available.");
  else m_error=tr("The music source is unavailable.");
  emit statusChanged();
}
void MusicCatalog::refresh(){
  readState();readActive();if(m_registryBusy)return;m_registryBusy=true;providers(QString(),++m_registryTicket,QString(),{},{});
}
void MusicCatalog::providers(const QString &token,quint64 ticket,const QString &snapshot,QVariantList rows,QSet<QString> tokens){
  if(tokens.size()>=32||rows.size()>1024){m_registryBusy=false;fail(QStringLiteral("Unavailable"));return;}
  if(!token.isEmpty()&&tokens.contains(token)){m_registryBusy=false;fail(QStringLiteral("Conflict"));return;}tokens.insert(token);
  auto message=call(QStringLiteral("/org/arinn/Daemon1"),"org.arinn.Daemon1","Providers");message<<token;
  auto *pending=new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(message,3000),this);
  connect(pending,&QDBusPendingCallWatcher::finished,this,[this,ticket,snapshot,rows,tokens](auto *watch)mutable{
    const auto reply=watch->reply();watch->deleteLater();if(ticket!=m_registryTicket)return;
    if(reply.type()==QDBusMessage::ErrorMessage||reply.signature()!=QStringLiteral("(sa(sssssoasssststttt)s)")){m_registryBusy=false;fail(code(reply));return;}
    const auto arg=qvariant_cast<QDBusArgument>(reply.arguments().first());QString seen,next;arg.beginStructure();arg>>seen;
    if(!snapshot.isEmpty()&&snapshot!=seen){m_registryBusy=false;fail(QStringLiteral("Conflict"));return;}
    arg.beginArray();while(!arg.atEnd()){
      QString id,plugin,source,name,version,runtime,backend,generation,error;QStringList capabilities;QDBusObjectPath endpoint;qulonglong revision,errorAt,recovered,restarts,cumulative;
      arg.beginStructure();arg>>id>>plugin>>source>>name>>version>>endpoint>>capabilities>>runtime>>backend>>generation>>revision>>error>>errorAt>>recovered>>restarts>>cumulative;arg.endStructure();
      if(capabilities.contains(QStringLiteral("music")))rows.append(QVariantMap{{"id",id},{"name",name},{"path",endpoint.path()},{"generation",generation},{"ready",runtime==QStringLiteral("ready")}});
    }arg.endArray();arg>>next;arg.endStructure();
    if(!next.isEmpty()){providers(next,ticket,seen,rows,tokens);return;}m_registryBusy=false;
    if(m_sources!=rows){m_sources=rows;emit sourcesChanged();emit activeChanged();}
    QString selected=m_sourceId;bool present=false;for(const auto &row:rows)if(row.toMap().value("id").toString()==selected)present=true;
    if(!present)selected=rows.isEmpty()?QString():rows.first().toMap().value("id").toString();
    choose(selected);
  });
}
void MusicCatalog::choose(const QString &id){
  QVariantMap selected;for(const auto &row:m_sources)if(row.toMap().value("id")==id){selected=row.toMap();break;}
  const auto path=selected.value("path").toString(),generation=selected.value("generation").toString();
  if(m_sourceId==id&&m_path==path&&m_generation==generation)return;
  m_uncertainStart=false;
  ++m_epoch;++m_pageTicket;++m_stateTicket;m_pageBusy=m_stateBusy=m_activeBusy=false;m_sourceId=id;m_path=path;m_generation=generation;m_facet.clear();m_catalogueRevision=0;m_entries.clear();m_state.clear();m_next.clear();m_error.clear();m_stateError=false;m_parent.clear();
  emit sourcesChanged();emit entriesChanged();emit stateChanged();emit statusChanged();emit browseReset();readState();
}
void MusicCatalog::readState(){
  if(m_path.isEmpty()||m_generation.isEmpty()||m_stateBusy||(m_commandBusy&&!m_commandWaitingRefresh))return;m_stateBusy=true;
  const auto epoch=m_epoch,ticket=++m_stateTicket;auto message=call(m_path,"org.arinn.Music1","GetState");message<<m_generation;
  auto *pending=new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(message,3500),this);
  connect(pending,&QDBusPendingCallWatcher::finished,this,[this,epoch,ticket](auto *watch){const auto reply=watch->reply();watch->deleteLater();if(epoch!=m_epoch||ticket!=m_stateTicket)return;m_stateBusy=false;
    const auto json=object(reply);if(json.isEmpty()||!json.contains("revision")||!json.value("actions").isArray()){fail(code(reply));m_stateError=true;return;}
    // A transient state error clears once the state reads again.
    if(m_stateError){m_stateError=false;m_error.clear();}
    const auto revision=json.value("catalogue_revision").toVariant().toULongLong();
    m_state=imageState(json.toVariantMap(),m_sourceId,m_path,m_generation);emit stateChanged();emit statusChanged();
    const auto facets=json.value("facets").toArray();
    // A source without facets (an account that is not connected, or one
    // without a catalogue) has nothing to page: asking only yields
    // Unsupported under the Connect prompt (review of #710).
    if(facets.isEmpty()){m_catalogueRevision=revision;if(!m_entries.isEmpty()||!m_next.isEmpty()){m_entries.clear();m_next.clear();emit entriesChanged();emit statusChanged();}return;}
    if(m_catalogueRevision!=revision){m_catalogueRevision=revision;bool found=false;for(const auto &facet:facets)if(facet.toString()==m_facet)found=true;if(!found)m_facet=facets.first().toString();readPage(false);}
  });
}
void MusicCatalog::browse(const QString &facet,const QString &parent,const QString &search){
  if(!QStringList{"tracks","albums","artists","playlists","selections"}.contains(facet)||search.toUtf8().size()>1024)return;
  if(m_facet!=facet || m_parent!=parent || m_search!=search){m_entries.clear();emit entriesChanged();}
  m_facet=facet;m_parent=parent;m_search=search;readPage(false);
}
void MusicCatalog::loadMore(){if(!m_pageBusy&&!m_next.isEmpty())readPage(true);}
void MusicCatalog::readPage(bool append){
  if(m_path.isEmpty()||m_generation.isEmpty())return;
  const auto epoch=m_epoch,ticket=++m_pageTicket;QString token=append?m_next:QString();
  if(!append){m_pageTokens.clear();m_pageRevision=0;}if(!token.isEmpty()&&m_pageTokens.contains(token)){fail(QStringLiteral("Conflict"));return;}
  m_pageBusy=true;emit statusChanged();auto message=call(m_path,"org.arinn.Music1","GetPage");
  const QJsonObject query{{"facet",m_facet},{"parent",m_parent},{"search",m_search},{"token",token}};message<<m_generation<<QString::fromUtf8(QJsonDocument(query).toJson(QJsonDocument::Compact));
  auto *pending=new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(message,5500),this);
  connect(pending,&QDBusPendingCallWatcher::finished,this,[this,epoch,ticket,append,token](auto *watch){const auto reply=watch->reply();watch->deleteLater();if(epoch!=m_epoch||ticket!=m_pageTicket)return;m_pageBusy=false;
    const auto json=object(reply);const auto revision=json.value("revision").toVariant().toULongLong();
    if(json.isEmpty()||json.value("generation").toString()!=m_generation||!json.value("entries").isArray()||revision==0||(append&&revision!=m_pageRevision)){fail(code(reply));return;}
    auto entries=append?m_entries:QVariantList();QSet<QString> ids;for(const auto &row:entries)ids.insert(row.toMap().value("id").toString());
    const auto page=json.value("entries").toArray();if(page.size()>100||entries.size()+page.size()>100000){fail(QStringLiteral("Unavailable"));return;}
    for(const auto &item:page){const auto row=imageEntity(item.toObject().toVariantMap(),m_sourceId,m_path,m_generation);const auto id=row.value("id").toString();if(id.isEmpty()||ids.contains(id)){fail(QStringLiteral("Conflict"));return;}ids.insert(id);entries.append(row);}
    m_entries=entries;m_pageRevision=revision;m_next=json.value("next_token").toString();if(!token.isEmpty())m_pageTokens.insert(token);m_error.clear();emit entriesChanged();emit statusChanged();
  });
}
QString MusicCatalog::activeSourceName()const{for(const auto &row:m_sources){const auto source=row.toMap();if(source.value("id").toString()==m_activeId)return source.value("name").toString();}return {};}
void MusicCatalog::readActive(){
  if(m_activeBusy)return;m_activeBusy=true;const auto epoch=m_epoch;
  auto message=call(QStringLiteral("/org/arinn/Daemon1"),"org.arinn.MusicController1","GetActive");
  auto *pending=new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(message,3000),this);
  connect(pending,&QDBusPendingCallWatcher::finished,this,[this,epoch](auto *watch){const auto reply=watch->reply();watch->deleteLater();if(epoch!=m_epoch)return;m_activeBusy=false;
    if(reply.type()==QDBusMessage::ErrorMessage||reply.signature()!=QStringLiteral("s")){if(m_commandWaitingRefresh){m_commandBusy=m_commandWaitingRefresh=false;emit statusChanged();}return;}
    if(m_commandWaitingRefresh){m_commandBusy=m_commandWaitingRefresh=false;emit statusChanged();}
    if(reply.arguments().first().toString()==QStringLiteral("null")){m_active.clear();m_activeId.clear();m_activeGeneration.clear();emit activeChanged();return;}
    const auto json=object(reply);if(json.isEmpty())return;
    m_active=json.value("state").toObject().toVariantMap();m_activeId=json.value("provider_id").toString();m_activeGeneration=json.value("generation").toString();
    QString path;for(const auto &row:m_sources){const auto source=row.toMap();if(source.value("id").toString()==m_activeId && source.value("generation").toString()==m_activeGeneration){path=source.value("path").toString();break;}}
    m_active=imageState(m_active,m_activeId,path,m_activeGeneration);
    if(m_uncertainStart && m_activeId==m_startProvider && m_activeGeneration==m_startGeneration
       && !m_active.value("session").toString().isEmpty() && m_active.value("session").toString()!=m_startSession
       && m_active.value("error").isNull() && QStringList{"loading","playing","paused"}.contains(m_active.value("playback").toString())) {
      m_uncertainStart=false;m_error.clear();emit statusChanged();
    }
    emit activeChanged();
  });
}
void MusicCatalog::command(const QString &action,const QString &entity,const QString &occurrence,qint64 positionUs,const QString &text){send(action,entity,occurrence,positionUs,false,text);}
void MusicCatalog::transport(const QString &action,const QString &entity,const QString &occurrence,qint64 positionUs,const QString &text){send(action,entity,occurrence,positionUs,true,text);}
void MusicCatalog::send(const QString &action,const QString &entity,const QString &occurrence,qint64 positionUs,bool active,const QString &text){
  if(m_commandBusy||positionUs<0)return;
  auto endpoint=m_path,generation=m_generation;auto state=m_state;
  if(active){
    bool found=false;
    for(const auto &row:m_sources){const auto source=row.toMap();if(source.value("id").toString()==m_activeId){if(source.value("generation").toString()!=m_activeGeneration)return;endpoint=source.value("path").toString();generation=m_activeGeneration;state=m_active;found=true;break;}}
    if(!found)return;
  }
  if(endpoint.isEmpty()||generation.isEmpty()||!state.value("actions").toStringList().contains(action))return;
  m_uncertainStart=false;
  m_commandBusy=true;m_commandWaitingRefresh=false;m_error.clear();++m_stateTicket;m_stateBusy=false;emit statusChanged();const auto epoch=m_epoch,commandTicket=++m_commandTicket;
  const QJsonObject payload{{"request_id",QUuid::createUuid().toString(QUuid::WithoutBraces)},{"action",action},{"session",state.value("session").toString()},{"entity",entity},{"occurrence",occurrence},{"position_us",positionUs},{"replaces_provider",m_activeId},{"replaces_session",m_active.value("session").toString()},{"text",text}};
  auto message=call(endpoint,"org.arinn.Music1","Command");message<<generation<<QString::fromUtf8(QJsonDocument(payload).toJson(QJsonDocument::Compact));
  const auto provider=active?m_activeId:m_sourceId,oldSession=state.value("session").toString();
  auto *pending=new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(message,5500),this);
  connect(pending,&QDBusPendingCallWatcher::finished,this,[this,epoch,active,commandTicket,action,provider,generation,oldSession](auto *watch){const auto reply=watch->reply();watch->deleteLater();if(commandTicket!=m_commandTicket)return;if(epoch!=m_epoch){m_commandBusy=false;emit statusChanged();readActive();return;}
    const auto json=object(reply);if(json.isEmpty()){
      const auto failure=code(reply);
      if((action=="play" || action=="radio") && (failure=="Timeout" || failure=="NoReply")) {
        m_uncertainStart=true;m_startProvider=provider;m_startGeneration=generation;m_startSession=oldSession;
      }
      fail(failure);m_commandWaitingRefresh=true;m_stateBusy=false;++m_stateTicket;readState();readActive();return;}
    m_commandBusy=false;if(!active){m_state=imageState(json.toVariantMap(),m_sourceId,m_path,m_generation);emit stateChanged();}emit statusChanged();readActive();
  });
}

QString MusicCatalog::sessionEntryId() { return QStringLiteral("arinn-music-session"); }

QVariantMap MusicCatalog::nowPlaying() const {
  const auto current = m_active.value(QStringLiteral("current")).toMap();
  const auto playback = m_active.value(QStringLiteral("playback")).toString();
  const bool playing = playback == QStringLiteral("playing");
  if (current.isEmpty() || (!playing && playback != QStringLiteral("paused")))
    return {};
  const auto track = current.value(QStringLiteral("track")).toMap();
  const auto actions = m_active.value(QStringLiteral("actions")).toStringList();
  return {{QStringLiteral("id"), sessionEntryId()},
          {QStringLiteral("kind"), QStringLiteral("music")},
          {QStringLiteral("title"), track.value(QStringLiteral("title"))},
          {QStringLiteral("subtitle"), track.value(QStringLiteral("artist"))},
          {QStringLiteral("state"), playback},
          // Not tied to a command in flight: the row would lose focus while
          // it waits, and the catalogue drops a second press anyway.
          {QStringLiteral("playPause"),
           actions.contains(playing ? QStringLiteral("pause") : QStringLiteral("resume"))}};
}

void MusicCatalog::togglePlayback() {
  const auto playback = m_active.value(QStringLiteral("playback")).toString();
  if (playback == QStringLiteral("playing"))
    transport(QStringLiteral("pause"));
  else if (playback == QStringLiteral("paused"))
    transport(QStringLiteral("resume"));
}

QString MusicCatalog::playbackProblem() const {
  const auto error = m_active.value(QStringLiteral("error")).toString();
  const auto playback = m_active.value(QStringLiteral("playback")).toString();
  // A provider keeps its last error after it skipped to the next track: a
  // session that plays, is paused or loads again has recovered.
  if (error.isEmpty() || QStringList{QStringLiteral("playing"), QStringLiteral("paused"),
                                     QStringLiteral("loading")}.contains(playback))
    return {};
  if (error == QStringLiteral("NotFound"))
    return tr("The track was not found in this source.");
  if (error == QStringLiteral("PermissionDenied"))
    return tr("This source no longer allows playing the track. Sign in again in Settings.");
  if (error == QStringLiteral("Timeout"))
    return tr("The music source did not respond in time.");
  if (error == QStringLiteral("Unsupported"))
    return tr("This track cannot be played here.");
  return tr("The track could not be played.");
}

QString MusicCatalog::notice() const {
  if (!m_error.isEmpty())
    return m_error;
  if (m_state.value(QStringLiteral("account")).toString() == QStringLiteral("restricted"))
    return tr("Music access was denied.");
  const auto problem = playbackProblem();
  if (!problem.isEmpty())
    return tr("Playback stopped. %1").arg(problem);
  // A degraded source still browses; some of its content is missing.
  if (m_state.value(QStringLiteral("backend")).toString() == QStringLiteral("degraded")
      && !m_state.value(QStringLiteral("error")).toString().isEmpty()
      && !m_state.value(QStringLiteral("catalogue_loading")).toBool())
    return tr("Part of this source is unavailable. Refresh to try again.");
  return {};
}

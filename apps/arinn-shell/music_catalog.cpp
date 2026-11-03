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
  auto *watch=new QDBusServiceWatcher(service,QDBusConnection::sessionBus(),QDBusServiceWatcher::WatchForOwnerChange,this);
  connect(watch,&QDBusServiceWatcher::serviceOwnerChanged,this,[this]{++m_commandTicket;++m_epoch;++m_pageTicket;++m_stateTicket;++m_registryTicket;m_registryBusy=m_pageBusy=m_stateBusy=m_commandBusy=m_activeBusy=m_commandWaitingRefresh=false;m_path.clear();m_generation.clear();m_sources.clear();m_entries.clear();m_state.clear();m_active.clear();m_activeId.clear();m_activeGeneration.clear();m_next.clear();emit activeChanged();emit sourcesChanged();emit entriesChanged();emit stateChanged();emit statusChanged();refresh();});
  m_timer.setInterval(1000);connect(&m_timer,&QTimer::timeout,this,&MusicCatalog::refresh);m_timer.start();QTimer::singleShot(0,this,&MusicCatalog::refresh);
}
QString MusicCatalog::sectionTitle()const{return tr("Music");}
QVariantList MusicCatalog::homeEntries()const {
  const auto current=m_active.value("current").toMap();if(current.isEmpty())return {};
  const auto track=current.value("track").toMap();const auto playback=m_active.value("playback").toString();
  const bool playing=playback==QStringLiteral("playing"),paused=playback==QStringLiteral("paused");
  const auto status=playing?tr("Playing"):paused?tr("Paused"):playback==QStringLiteral("loading")?tr("Loading…"):tr("Stopped");
  return {QVariantMap{{"id",QStringLiteral("arinn-music-session")},{"kind",QStringLiteral("music")},{"title",track.value("title")},{"subtitle",track.value("artist")},{"artwork",track.value("image")},{"statusText",tr("Music · %1").arg(status)},{"secondaryIcon",playing?QStringLiteral("pause"):QStringLiteral("play")},{"secondaryText",playing?tr("Pause"):tr("Play")},{"secondaryEnabled",playing||paused},{"busy",m_commandBusy}}};
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
  auto message=call(QStringLiteral("/org/arinn/Daemon1"),"org.arinn.Daemon1","GetProviders");message<<token;
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
    QString selected=m_sourceId;if(selected.isEmpty()&&!rows.isEmpty())selected=rows.first().toMap().value("id").toString();
    choose(selected);
  });
}
void MusicCatalog::choose(const QString &id){
  QVariantMap selected;for(const auto &row:m_sources)if(row.toMap().value("id")==id){selected=row.toMap();break;}
  const auto path=selected.value("path").toString(),generation=selected.value("generation").toString();
  if(m_sourceId==id&&m_path==path&&m_generation==generation)return;
  ++m_epoch;++m_pageTicket;++m_stateTicket;m_pageBusy=m_stateBusy=m_activeBusy=false;m_sourceId=id;m_path=path;m_generation=generation;m_facet.clear();m_catalogueRevision=0;m_entries.clear();m_state.clear();m_next.clear();m_error.clear();m_parent.clear();
  emit sourcesChanged();emit entriesChanged();emit stateChanged();emit statusChanged();readState();
}
void MusicCatalog::readState(){
  if(m_path.isEmpty()||m_generation.isEmpty()||m_stateBusy||(m_commandBusy&&!m_commandWaitingRefresh))return;m_stateBusy=true;
  const auto epoch=m_epoch,ticket=++m_stateTicket;auto message=call(m_path,"org.arinn.Music1","GetState");message<<m_generation;
  auto *pending=new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(message,3500),this);
  connect(pending,&QDBusPendingCallWatcher::finished,this,[this,epoch,ticket](auto *watch){const auto reply=watch->reply();watch->deleteLater();if(epoch!=m_epoch||ticket!=m_stateTicket)return;m_stateBusy=false;
    const auto json=object(reply);if(json.isEmpty()||!json.contains("revision")||!json.value("actions").isArray()){fail(code(reply));return;}
    const auto revision=json.value("catalogue_revision").toVariant().toULongLong();
    m_state=imageState(json.toVariantMap(),m_sourceId,m_path,m_generation);emit stateChanged();emit statusChanged();
    if(m_catalogueRevision!=revision){m_catalogueRevision=revision;const auto facets=json.value("facets").toArray();bool found=false;for(const auto &facet:facets)if(facet.toString()==m_facet)found=true;if(!found&&!facets.isEmpty())m_facet=facets.first().toString();readPage(false);}
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
    m_active=imageState(m_active,m_activeId,path,m_activeGeneration);emit activeChanged();
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
  m_commandBusy=true;m_commandWaitingRefresh=false;m_error.clear();++m_stateTicket;m_stateBusy=false;emit statusChanged();const auto epoch=m_epoch,commandTicket=++m_commandTicket;
  const QJsonObject payload{{"request_id",QUuid::createUuid().toString(QUuid::WithoutBraces)},{"action",action},{"session",state.value("session").toString()},{"entity",entity},{"occurrence",occurrence},{"position_us",positionUs},{"replaces_provider",m_activeId},{"replaces_session",m_active.value("session").toString()},{"text",text}};
  auto message=call(endpoint,"org.arinn.Music1","Command");message<<generation<<QString::fromUtf8(QJsonDocument(payload).toJson(QJsonDocument::Compact));
  auto *pending=new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(message,5500),this);
  connect(pending,&QDBusPendingCallWatcher::finished,this,[this,epoch,active,commandTicket](auto *watch){const auto reply=watch->reply();watch->deleteLater();if(commandTicket!=m_commandTicket)return;if(epoch!=m_epoch){m_commandBusy=false;emit statusChanged();readActive();return;}
    const auto json=object(reply);if(json.isEmpty()){fail(code(reply));m_commandWaitingRefresh=true;m_stateBusy=false;++m_stateTicket;readState();readActive();return;}
    m_commandBusy=false;if(!active){m_state=imageState(json.toVariantMap(),m_sourceId,m_path,m_generation);emit stateChanged();}emit statusChanged();readActive();
  });
}

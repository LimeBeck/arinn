/*
 * SPDX-FileCopyrightText: 2026 Arinn contributors
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#pragma once
#include "flow_trace.h"
#include <QHash>
#include <QObject>
#include <QSet>
#include <QTimer>
#include <QVariantList>
#include <QVariantMap>
#include <memory>

// Reads org.arinn.Downloads1 for the shell (AC-17, spec §5.4, ADR-0067).
// Screens see presentation rows only; Downloads1 carries no file paths.
// Pause, Resume, StartNow and Retry are Operation1 commands.
class DownloadCatalog : public QObject {
  Q_OBJECT
  // loading, populated, empty, error, unavailable (DownloadsView.viewState)
  Q_PROPERTY(QString downloadsState READ downloadsState NOTIFY catalogChanged)
  Q_PROPERTY(QVariantList jobs READ jobs NOTIFY catalogChanged)
  // Home's status chip (#367): whether anything is downloading, and the
  // aggregate progress 0–1 of active downloads, or -1 when none knows it.
  Q_PROPERTY(bool downloading READ downloading NOTIFY catalogChanged)
  Q_PROPERTY(double downloadProgress READ downloadProgress NOTIFY catalogChanged)
public:
  explicit DownloadCatalog(QObject *parent = nullptr);
  QString downloadsState() const;
  QVariantList jobs() const { return m_jobs; }
  bool downloading() const;
  double downloadProgress() const;
  // action: pause, resume, start_now or retry (the view's action names).
  Q_INVOKABLE void request(const QString &id, const QString &action);
  // {provider, local} of the first library item an imported download
  // produced, or an empty map.
  Q_INVOKABLE QVariantMap watchTarget(const QString &id) const;
public slots:
  void refresh();
signals:
  void catalogChanged();
  // Once per download per shell session, when its import completes.
  void downloadImported(const QString &message);

private slots:
  void stateChanged(const QString &provider, const QString &capability,
                    const QString &generation, qulonglong revision);

private:
  struct Row {
    QString id;
    QString providerId;
    QString path;
    QString generation;
    QString localId;
    QString title;
    QString state;
    quint64 completedBytes = 0;
    bool hasTotal = false;
    quint64 totalBytes = 0;
    quint64 rate = 0;
    qint64 eta = -1;
    bool hasProgress = false;
    double progress = 0;
    QStringList actions;
    QString errorCode;
    QString errorMessage;
    QString importState;
    QString refProvider;
    QString refLocal;
    // The provider's backend is unavailable: the cached row stays readable
    // and no command is offered (ADR-0067).
    bool backendUnavailable = false;
  };
  struct Catalog;
  void readProviders(const std::shared_ptr<Catalog> &catalog,
                     const QString &token = {});
  void readDownloads(const std::shared_ptr<Catalog> &catalog, int index,
                     const QString &token = {});
  void failProvider(const std::shared_ptr<Catalog> &catalog, int index,
                    bool unavailable);
  void finishCatalog(const std::shared_ptr<Catalog> &catalog, bool success);
  void watchOperation(const QString &id, quint64 ticket, const QString &action,
                      const QString &objectPath, quint64 epoch);
  void finishCommand(const QString &id, quint64 ticket, const QString &action,
                     const QString &result);
  void announceImports();
  void publish();
  const Row *findId(const QString &id) const;
  static bool validRow(const Row &row, const QStringList &outputs, int refs,
                       bool refsValid);
  QVariantList m_jobs;
  QList<Row> m_rows;
  // Presentation id → the job state when the failure was reported, and the
  // message. The message stays until the job's state changes.
  // A failed command's message, shown on its row until the download changes
  // state or provider generation.
  struct Failure {
    QString state;
    QString generation;
    QString message;
  };
  QHash<QString, Failure> m_failures;
  // The command in flight per presentation id: its ticket and the provider
  // generation it was sent to. A new generation drops it (review of #310).
  struct Command {
    quint64 ticket = 0;
    QString generation;
  };
  QHash<QString, Command> m_busy;
  quint64 m_ticket = 0;
  // Reads in a row whose snapshot a newer StateChanged already superseded.
  int m_superseded = 0;
  // Read timing for diagnostics (ADR-0093): a read is reported when it
  // changes the state the screen shows or fails, not on every poll.
  void traceRead();
  FlowTrace::Flow m_readFlow;
  QString m_tracedState;
  // Session memory for the completion toast: whether each download was last
  // seen imported, and which downloads were already announced.
  QHash<QString, bool> m_imported;
  QSet<QString> m_announced;
  bool m_connected = false, m_loaded = false, m_error = false,
       m_unreachable = false, m_refreshing = false, m_dirty = false;
  quint64 m_epoch = 0;
  QTimer m_timer;
};

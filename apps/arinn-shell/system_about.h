// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QDate>
#include <QObject>
#include <QString>
#include <QVariantList>
#include <QVariantMap>

// Facts for Settings → System → About (#742). The shell's build id comes
// from build_identity (ADR-0097). The daemon's version and build id come
// from the existing Daemon1.BuildInfo, and installed providers from
// Daemon1.Providers. Everything else is read in the shell: no new D-Bus
// field. A fixture replaces the live facts for tests.
class SystemAbout : public QObject {
  Q_OBJECT
  Q_PROPERTY(QString shellShort READ shellShort NOTIFY changed)
  Q_PROPERTY(QString shellBuildId READ shellBuildId NOTIFY changed)
  Q_PROPERTY(bool daemonAvailable READ daemonAvailable NOTIFY changed)
  Q_PROPERTY(QString daemonShort READ daemonShort NOTIFY changed)
  Q_PROPERTY(QString daemonBuildId READ daemonBuildId NOTIFY changed)
  Q_PROPERTY(bool daemonMismatch READ daemonMismatch NOTIFY changed)
  Q_PROPERTY(QString channelDate READ channelDate NOTIFY changed)
  Q_PROPERTY(QString providerDetail READ providerDetail NOTIFY changed)
  // -1 until a listing answers. Zero is a daemon with no named providers.
  Q_PROPERTY(int providerCount READ providerCount NOTIFY changed)
  Q_PROPERTY(QString osName READ osName NOTIFY changed)
  Q_PROPERTY(QString kernel READ kernel NOTIFY changed)
  Q_PROPERTY(QString plasma READ plasma NOTIFY changed)
  Q_PROPERTY(QString qtVersion READ qtVersion NOTIFY changed)
  Q_PROPERTY(QString session READ session NOTIFY changed)
  Q_PROPERTY(QString display READ displayText NOTIFY changed)
  Q_PROPERTY(QString audio READ audio NOTIFY changed)
  Q_PROPERTY(QString connection READ connection NOTIFY changed)
  Q_PROPERTY(QString ipAddress READ ipAddress NOTIFY changed)
  Q_PROPERTY(QString storage READ storage NOTIFY changed)
  Q_PROPERTY(QString deviceName READ deviceName WRITE setDeviceName NOTIFY changed)
  Q_PROPERTY(QString fileName READ fileName NOTIFY changed)

public:
  explicit SystemAbout(QObject *parent = nullptr);

  QString shellShort() const { return m_shellShort; }
  QString shellBuildId() const { return m_shellBuildId; }
  bool daemonAvailable() const { return m_daemonAvailable; }
  QString daemonShort() const { return m_daemonShort; }
  QString daemonBuildId() const { return m_daemonBuildId; }
  bool daemonMismatch() const { return m_daemonMismatch; }
  QString channelDate() const { return m_channelDate; }
  QString providerDetail() const { return m_providerDetail; }
  int providerCount() const { return m_providerCount; }
  QString osName() const { return m_os; }
  QString kernel() const { return m_kernel; }
  QString plasma() const { return m_plasma; }
  QString qtVersion() const { return m_qt; }
  QString session() const { return m_session; }
  QString displayText() const { return m_display; }
  QString audio() const { return m_audio; }
  QString connection() const { return m_connection; }
  QString ipAddress() const { return m_ip; }
  QString storage() const { return m_storage; }
  QString deviceName() const { return m_deviceName; }
  QString fileName() const { return m_fileName; }

  // "version · commit" from a build id. `version` wins over the id's own
  // version when BuildInfo sends both; an empty commit leaves the version.
  static QString shortBuild(const QString &buildId, const QString &version = {});
  // "master · Nov 3, 2026", or whichever half is known, in the default locale.
  static QString formatChannelDate(const QString &channel, const QDate &date);

  void setDeviceName(const QString &name);
  // Tests: stop reading the machine and the bus, and show `facts` instead.
  void useFixture(const QVariantMap &facts);
  void setDownloadsDirectory(const QString &path) { m_downloadsDir = path; }

  QString report(bool includePrivate) const;
  // 0 clipboard and file, 1 file failed, 2 clipboard failed, 3 both failed.
  Q_INVOKABLE int copyReport(bool includePrivate);
  Q_INVOKABLE void refreshLocal();
  Q_INVOKABLE void refreshFileName();

signals:
  void changed();

private slots:
  void readDaemon();

private:
  void requestBuild();
  void requestProviders(const QString &token);
  void markDaemonDown();
  void applyProviders(const QVariantList &providers);
  bool writeReport(const QString &text) const;
  bool copyClipboard(const QString &text) const;
  QString downloadsPath() const;

  bool m_fixture = false;
  quint64 m_lifetime = 0;
  quint64 m_buildRequest = 0;
  quint64 m_providerTicket = 0;
  int m_providerPages = 0;
  QVariantList m_providerAccum;
  QString m_shellShort;
  QString m_shellBuildId;
  bool m_daemonAvailable = false;
  bool m_daemonForeign = false;
  QString m_daemonShort;
  QString m_daemonBuildId;
  bool m_daemonMismatch = false;
  QString m_channelDate;
  QString m_providerDetail;
  int m_providerCount = -1;
  QString m_os;
  QString m_kernel;
  QString m_plasma;
  QString m_qt;
  QString m_session;
  QString m_display;
  QString m_audio;
  QString m_connection;
  QString m_ip;
  QString m_storage;
  QString m_deviceName;
  QString m_fileName;
  QString m_downloadsDir;
  class QTimer *m_retry = nullptr;
  int m_attempt = 0;
};

// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "system_about.h"
#include "build_identity.h"
#include <QAudioDevice>
#include <QClipboard>
#include <QCoreApplication>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QLocale>
#include <QMediaDevices>
#include <QRegularExpression>
#include <QSaveFile>
#include <QScreen>
#include <QStandardPaths>
#include <QStorageInfo>
#include <QTimer>
#include <QtDBus>
#include <QtQml>
#include <cmath>
#include <limits>
#include <ifaddrs.h>
#include <arpa/inet.h>
#include <net/if.h>
#include <netinet/in.h>
#include <sys/utsname.h>

namespace {
const QString service = QStringLiteral("org.arinn.Daemon1");
const QString path = QStringLiteral("/org/arinn/Daemon1");
const QString interface = QStringLiteral("org.arinn.Daemon1");
constexpr auto providersPage = "(sa(sssssoasssststttt)s)";
const QList<int> retryDelays{1000, 5000};

QString unquote(QString value) {
  value = value.trimmed();
  if (value.size() >= 2 && value.front() == QLatin1Char('"') &&
      value.back() == QLatin1Char('"'))
    value = value.mid(1, value.size() - 2);
  return value;
}

QString readOsRelease() {
  QFile file(QStringLiteral("/etc/os-release"));
  if (!file.open(QIODevice::ReadOnly | QIODevice::Text))
    return {};
  QString pretty, name;
  while (!file.atEnd()) {
    const auto line = QString::fromUtf8(file.readLine()).trimmed();
    if (line.isEmpty() || line.startsWith(QLatin1Char('#')))
      continue;
    const auto eq = line.indexOf(QLatin1Char('='));
    if (eq <= 0)
      continue;
    const auto key = line.left(eq);
    const auto value = unquote(line.mid(eq + 1));
    if (key == QLatin1String("PRETTY_NAME"))
      pretty = value;
    else if (key == QLatin1String("NAME"))
      name = value;
  }
  return !pretty.isEmpty() ? pretty : name;
}

QString readKernel() {
  utsname buf{};
  if (uname(&buf) != 0)
    return {};
  return QString::fromLocal8Bit(buf.release);
}

// A packaged Plasma install records its version here. Absent means Unknown:
// the shell does not run plasmashell or talk to it (R-03).
QString readPlasma() {
  const QStringList paths = {
      QStringLiteral("/usr/lib/cmake/Plasma/PlasmaConfigVersion.cmake"),
      QStringLiteral("/usr/lib64/cmake/Plasma/PlasmaConfigVersion.cmake"),
      QStringLiteral(
          "/usr/lib/x86_64-linux-gnu/cmake/Plasma/PlasmaConfigVersion.cmake")};
  const QRegularExpression version(
      QStringLiteral("set\\(PACKAGE_VERSION\\s+\"([0-9][^\"]*)\"\\)"));
  for (const auto &path : paths) {
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly))
      continue;
    const auto match = version.match(QString::fromUtf8(file.read(4096)));
    if (match.hasMatch())
      return match.captured(1);
  }
  return {};
}

QString readSession() {
  auto desktop = qEnvironmentVariable("XDG_CURRENT_DESKTOP");
  const auto colon = desktop.indexOf(QLatin1Char(':'));
  if (colon >= 0)
    desktop.truncate(colon);
  desktop = desktop.trimmed();
  const auto type = qEnvironmentVariable("XDG_SESSION_TYPE").trimmed().toLower();
  QString sessionType;
  if (type == QLatin1String("wayland"))
    sessionType = QStringLiteral("Wayland");
  else if (type == QLatin1String("x11"))
    sessionType = QStringLiteral("X11");
  else if (!type.isEmpty() && type != QLatin1String("tty") &&
           type != QLatin1String("unspecified"))
    sessionType = type;
  if (!desktop.isEmpty() && !sessionType.isEmpty())
    return desktop + QStringLiteral(" · ") + sessionType;
  return !desktop.isEmpty() ? desktop : sessionType;
}

QString readDisplay() {
  const auto *screen = QGuiApplication::primaryScreen();
  if (!screen)
    return {};
  return SystemAbout::formatDisplay(screen->size(), screen->devicePixelRatio(),
                                    screen->refreshRate());
}

QString readAudio() {
  // Offscreen tests have no output device; asking can stall the platform.
  if (QGuiApplication::platformName() == QLatin1String("offscreen"))
    return {};
  const auto device = QMediaDevices::defaultAudioOutput();
  return device.isNull() ? QString() : device.description();
}

QString defaultRouteInterface() {
  QFile route(QStringLiteral("/proc/net/route"));
  if (!route.open(QIODevice::ReadOnly | QIODevice::Text))
    return {};
  bool header = true;
  while (!route.atEnd()) {
    const auto line = QString::fromUtf8(route.readLine()).trimmed();
    if (header) {
      header = false;
      continue;
    }
    const auto parts = line.split(QLatin1Char('\t'), Qt::SkipEmptyParts);
    // Iface, Destination, Gateway. 00000000 is the default route.
    if (parts.size() >= 2 && parts.at(1) == QLatin1String("00000000") &&
        parts.at(0) != QLatin1String("lo"))
      return parts.at(0);
  }
  return {};
}

QString connectionKind(const QString &iface) {
  if (iface.isEmpty())
    return {};
  if (QFileInfo::exists(QStringLiteral("/sys/class/net/%1/wireless").arg(iface)) ||
      QFileInfo::exists(QStringLiteral("/sys/class/net/%1/phy80211").arg(iface)))
    return QStringLiteral("wifi");
  return QStringLiteral("ethernet");
}

struct NetFact {
  QString iface;
  QString ip;
  QString kind;
};

NetFact readNetwork() {
  const auto preferred = defaultRouteInterface();
  ifaddrs *list = nullptr;
  if (getifaddrs(&list) != 0)
    return {};
  QString fallbackIface, fallbackIp, chosenIp;
  for (auto *ifa = list; ifa; ifa = ifa->ifa_next) {
    if (!ifa->ifa_addr || ifa->ifa_addr->sa_family != AF_INET)
      continue;
    if ((ifa->ifa_flags & IFF_LOOPBACK) || !(ifa->ifa_flags & IFF_UP))
      continue;
    const auto name = QString::fromLocal8Bit(ifa->ifa_name);
    if (name == QLatin1String("lo"))
      continue;
    char buf[INET_ADDRSTRLEN] = {};
    const auto *in = reinterpret_cast<sockaddr_in *>(ifa->ifa_addr);
    if (!inet_ntop(AF_INET, &in->sin_addr, buf, sizeof buf))
      continue;
    const auto ip = QString::fromLatin1(buf);
    if (!preferred.isEmpty() && name == preferred) {
      chosenIp = ip;
      break;
    }
    if (fallbackIp.isEmpty()) {
      fallbackIp = ip;
      fallbackIface = name;
    }
  }
  freeifaddrs(list);
  NetFact fact;
  fact.iface = !chosenIp.isEmpty() ? preferred : fallbackIface;
  fact.ip = !chosenIp.isEmpty() ? chosenIp : fallbackIp;
  fact.kind = connectionKind(fact.iface);
  return fact;
}

QString formatConnection(const QString &kind, const QString &iface) {
  if (iface.isEmpty())
    return {};
  const auto joiner = QStringLiteral(" · ");
  if (kind == QLatin1String("wifi"))
    return QCoreApplication::translate("SystemAbout", "Wi-Fi") + joiner + iface;
  if (kind == QLatin1String("ethernet"))
    return QCoreApplication::translate("SystemAbout", "Ethernet") + joiner + iface;
  return iface;
}

qint64 wholeGigabytes(qint64 bytes) {
  const qint64 gib = 1LL << 30;
  if (bytes <= 0)
    return 0;
  return (bytes + gib / 2) / gib;
}

QString formatStorage(const QString &directory) {
  const QStorageInfo info(directory);
  if (!info.isValid() || info.bytesTotal() <= 0)
    return {};
  const auto free = QCoreApplication::translate("SystemAbout", "%1 GB")
                        .arg(wholeGigabytes(info.bytesAvailable()));
  const auto total = QCoreApplication::translate("SystemAbout", "%1 GB")
                         .arg(wholeGigabytes(info.bytesTotal()));
  return QCoreApplication::translate("SystemAbout", "%1 of %2").arg(free, total);
}

bool noOwner(const QString &error) {
  return error == QLatin1String("org.freedesktop.DBus.Error.ServiceUnknown") ||
         error == QLatin1String("org.freedesktop.DBus.Error.NameHasNoOwner");
}
} // namespace

void registerSystemAbout() {
  qmlRegisterType<SystemAbout>("Arinn.Shell", 1, 0, "SystemAbout");
}
Q_COREAPP_STARTUP_FUNCTION(registerSystemAbout)

QString SystemAbout::shortBuild(const QString &buildId, const QString &version) {
  static const QRegularExpression re(QStringLiteral(
      R"(^(\d+\.\d+\.\d+)(?:\+g([0-9a-f]{12})(?:-dirty)?)?$)"));
  const auto match = re.match(buildId.trimmed());
  QString shown = version.trimmed();
  QString commit;
  if (match.hasMatch()) {
    if (shown.isEmpty())
      shown = match.captured(1);
    commit = match.captured(2);
  }
  if (shown.isEmpty())
    return buildId.trimmed();
  if (commit.isEmpty())
    return shown;
  return shown + QStringLiteral(" · ") + commit;
}

QString SystemAbout::formatChannelDate(const QString &channel, const QDate &date) {
  QStringList parts;
  const auto name = channel.trimmed();
  if (!name.isEmpty() && name != QLatin1String("HEAD"))
    parts.append(name);
  if (date.isValid()) {
    const auto locale = QLocale();
    const auto format = locale.language() == QLocale::Russian
                            ? QStringLiteral("d MMMM yyyy")
                            : QStringLiteral("MMM d, yyyy");
    parts.append(locale.toString(date, format));
  }
  return parts.join(QStringLiteral(" · "));
}

QString SystemAbout::formatDisplay(const QSize &logicalSize, qreal devicePixelRatio,
                                   qreal refreshRate) {
  if (logicalSize.width() <= 0 || logicalSize.height() <= 0 ||
      !std::isfinite(devicePixelRatio) || devicePixelRatio <= 0)
    return {};
  const QSizeF pixels = QSizeF(logicalSize) * devicePixelRatio;
  if (pixels.width() > std::numeric_limits<int>::max() ||
      pixels.height() > std::numeric_limits<int>::max())
    return {};
  // Qt screen sizes use device-independent coordinates. Keep a fractional
  // scale until the final conversion to whole device pixels (#832).
  const auto size = pixels.toSize();
  const auto hz = std::isfinite(refreshRate) && refreshRate > 1 &&
                          refreshRate < std::numeric_limits<int>::max()
                      ? qRound(refreshRate) : 0;
  if (hz > 1)
    return tr("%1 × %2 · %3 Hz").arg(size.width()).arg(size.height()).arg(hz);
  return tr("%1 × %2").arg(size.width()).arg(size.height());
}

SystemAbout::SystemAbout(QObject *parent) : QObject(parent) {
  m_retry = new QTimer(this);
  m_retry->setSingleShot(true);
  connect(m_retry, &QTimer::timeout, this, &SystemAbout::readDaemon);
  auto bus = QDBusConnection::sessionBus();
  bus.connect(service, path, interface, QStringLiteral("ProvidersChanged"), this,
              SLOT(readDaemon()));
  auto watcher = new QDBusServiceWatcher(
      service, bus, QDBusServiceWatcher::WatchForOwnerChange, this);
  connect(watcher, &QDBusServiceWatcher::serviceOwnerChanged, this,
          [this](const QString &, const QString &, const QString &next) {
            if (m_fixture)
              return;
            ++m_lifetime;
            m_retry->stop();
            m_attempt = 0;
            if (next.isEmpty()) {
              markDaemonDown();
              return;
            }
            readDaemon();
          });
  refreshFileName();
  refreshLocal();
}

void SystemAbout::setDeviceName(const QString &name) {
  if (name == m_deviceName)
    return;
  m_deviceName = name;
  emit changed();
}

void SystemAbout::useFixture(const QVariantMap &facts) {
  ++m_lifetime;
  m_retry->stop();
  m_fixture = true;
  const auto text = [&](const char *key) {
    return facts.value(QLatin1String(key)).toString();
  };
  m_shellBuildId = text("shellBuildId");
  m_shellShort = text("shellShort");
  if (m_shellShort.isEmpty())
    m_shellShort = shortBuild(m_shellBuildId);
  m_daemonAvailable = facts.value(QStringLiteral("daemonAvailable")).toBool();
  m_daemonForeign = facts.value(QStringLiteral("daemonForeign")).toBool();
  m_daemonBuildId = text("daemonBuildId");
  m_daemonShort = text("daemonShort");
  if (m_daemonShort.isEmpty() && !m_daemonBuildId.isEmpty())
    m_daemonShort = shortBuild(m_daemonBuildId, text("daemonVersion"));
  m_daemonMismatch = facts.value(QStringLiteral("daemonMismatch")).toBool();
  m_channelDate = text("channelDate");
  m_providerDetail = text("providerDetail");
  m_providerCount = facts.contains(QStringLiteral("providerCount"))
                        ? facts.value(QStringLiteral("providerCount")).toInt()
                        : -1;
  m_os = text("osName");
  m_kernel = text("kernel");
  m_plasma = text("plasma");
  m_qt = text("qtVersion");
  m_session = text("session");
  m_display = text("display");
  m_audio = text("audio");
  m_connection = text("connection");
  m_ip = text("ipAddress");
  m_storage = text("storage");
  m_deviceName = text("deviceName");
  if (!text("fileName").isEmpty())
    m_fileName = text("fileName");
  emit changed();
}

void SystemAbout::refreshFileName() {
  const auto stamp = QDateTime::currentDateTime().toString(QStringLiteral("yyyy-MM-dd-HHmm"));
  const auto name = QStringLiteral("arinn-about-%1.txt").arg(stamp);
  if (name == m_fileName)
    return;
  m_fileName = name;
  emit changed();
}

void SystemAbout::refreshLocal() {
  if (m_fixture)
    return;
  m_shellBuildId = BuildIdentity::shell();
  m_shellShort = shortBuild(m_shellBuildId);
  m_channelDate = formatChannelDate(QStringLiteral(ARINN_BUILD_CHANNEL),
                                    QDate::fromString(QStringLiteral(ARINN_BUILD_DATE),
                                                      Qt::ISODate));
  m_os = readOsRelease();
  m_kernel = readKernel();
  m_plasma = readPlasma();
  m_qt = QString::fromLatin1(qVersion());
  m_session = readSession();
  m_display = readDisplay();
  m_audio = readAudio();
  const auto net = readNetwork();
  m_connection = formatConnection(net.kind, net.iface);
  m_ip = net.ip;
  m_storage = formatStorage(downloadsPath());
  emit changed();
  readDaemon();
}

QString SystemAbout::downloadsPath() const {
  if (!m_downloadsDir.isEmpty())
    return m_downloadsDir;
  const auto location =
      QStandardPaths::writableLocation(QStandardPaths::DownloadLocation);
  if (!location.isEmpty())
    return location;
  return QDir::homePath() + QStringLiteral("/Downloads");
}

void SystemAbout::readDaemon() {
  if (m_fixture)
    return;
  m_retry->stop();
  requestBuild();
  requestProviders(QString());
}

void SystemAbout::markDaemonDown() {
  // Drop a listing that is still in flight: it belongs to the daemon that left.
  ++m_providerTicket;
  ++m_buildRequest;
  const bool was = m_daemonAvailable || m_daemonMismatch || !m_daemonShort.isEmpty() ||
                   !m_daemonBuildId.isEmpty() || m_providerCount >= 0 ||
                   !m_providerDetail.isEmpty();
  m_daemonAvailable = false;
  m_daemonForeign = false;
  m_daemonMismatch = false;
  m_daemonShort.clear();
  m_daemonBuildId.clear();
  m_providerDetail.clear();
  m_providerCount = -1;
  m_providerAccum.clear();
  if (was)
    emit changed();
}

void SystemAbout::requestBuild() {
  auto message = QDBusMessage::createMethodCall(service, path, interface,
                                                QStringLiteral("BuildInfo"));
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(message, 3000), this);
  const auto lifetime = m_lifetime;
  const auto request = ++m_buildRequest;
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, lifetime, request](QDBusPendingCallWatcher *watcher) {
            const auto reply = watcher->reply();
            watcher->deleteLater();
            if (m_fixture || lifetime != m_lifetime || request != m_buildRequest)
              return;
            if (reply.type() == QDBusMessage::ErrorMessage) {
              const auto error = reply.errorName();
              if (noOwner(error)) {
                markDaemonDown();
                return;
              }
              if (error == QLatin1String("org.freedesktop.DBus.Error.UnknownMethod")) {
                // A daemon older than BuildInfo is another build (ADR-0097).
                m_daemonAvailable = true;
                m_daemonForeign = true;
                m_daemonMismatch = true;
                m_daemonShort.clear();
                m_daemonBuildId.clear();
                m_attempt = 0;
                emit changed();
                return;
              }
              if (m_attempt < retryDelays.size())
                m_retry->start(retryDelays.at(m_attempt++));
              return;
            }
            if (reply.signature() != QLatin1String("ss") || reply.arguments().size() < 2) {
              if (m_attempt < retryDelays.size())
                m_retry->start(retryDelays.at(m_attempt++));
              return;
            }
            m_attempt = 0;
            const auto version = reply.arguments().at(0).toString();
            const auto buildId = reply.arguments().at(1).toString();
            m_daemonAvailable = true;
            m_daemonForeign = false;
            m_daemonBuildId = buildId;
            m_daemonShort = shortBuild(buildId, version);
            m_daemonMismatch = !buildId.isEmpty() &&
                               !BuildIdentity::same(BuildIdentity::shell(), buildId);
            emit changed();
          });
}

void SystemAbout::requestProviders(const QString &token) {
  if (m_fixture)
    return;
  if (token.isEmpty()) {
    m_providerAccum.clear();
    m_providerPages = 0;
    ++m_providerTicket;
  }
  if (++m_providerPages > 32) {
    applyProviders(m_providerAccum);
    return;
  }
  auto message =
      QDBusMessage::createMethodCall(service, path, interface, QStringLiteral("Providers"));
  message << token;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(message, 3000), this);
  const auto lifetime = m_lifetime;
  const auto ticket = m_providerTicket;
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, lifetime, ticket, token](QDBusPendingCallWatcher *watcher) {
            const auto reply = watcher->reply();
            watcher->deleteLater();
            if (m_fixture || lifetime != m_lifetime || ticket != m_providerTicket)
              return;
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != QLatin1String(providersPage) ||
                reply.arguments().isEmpty()) {
              if (reply.type() == QDBusMessage::ErrorMessage && noOwner(reply.errorName()))
                markDaemonDown();
              return;
            }
            const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
            QString snapshot, next;
            arg.beginStructure();
            arg >> snapshot;
            arg.beginArray();
            while (!arg.atEnd()) {
              QString id, plugin, source, name, version, runtime, backend, generation, error;
              QStringList capabilities;
              QDBusObjectPath endpoint;
              qulonglong revision = 0, errorAt = 0, recovered = 0, restarts = 0, cumulative = 0;
              arg.beginStructure();
              arg >> id >> plugin >> source >> name >> version >> endpoint >> capabilities >>
                  runtime >> backend >> generation >> revision >> error >> errorAt >>
                  recovered >> restarts >> cumulative;
              arg.endStructure();
              if (!name.trimmed().isEmpty())
                m_providerAccum.append(QVariantMap{{QStringLiteral("name"), name.trimmed()},
                                                   {QStringLiteral("version"), version.trimmed()}});
            }
            arg.endArray();
            arg >> next;
            arg.endStructure();
            if (!next.isEmpty() && next != token)
              requestProviders(next);
            else
              applyProviders(m_providerAccum);
          });
}

void SystemAbout::applyProviders(const QVariantList &providers) {
  QStringList parts;
  for (const auto &value : providers) {
    const auto row = value.toMap();
    const auto name = row.value(QStringLiteral("name")).toString();
    const auto version = row.value(QStringLiteral("version")).toString();
    if (name.isEmpty())
      continue;
    parts.append(version.isEmpty() ? name : name + QStringLiteral(" ") + version);
  }
  const auto detail = parts.join(QStringLiteral(" · "));
  if (parts.size() == m_providerCount && detail == m_providerDetail)
    return;
  m_providerCount = parts.size();
  m_providerDetail = detail;
  emit changed();
}

QString SystemAbout::report(bool includePrivate) const {
  const auto value = [this](const QString &text) {
    return text.isEmpty() ? tr("Unknown") : text;
  };
  const auto line = [](const QString &label, const QString &text) {
    return label + QStringLiteral(": ") + text;
  };
  QStringList lines;
  lines << tr("Arinn");
  lines << line(tr("Arinn version"), value(m_shellShort));
  if (!m_shellBuildId.isEmpty())
    lines << tr("Build %1").arg(m_shellBuildId);
  if (!m_daemonAvailable) {
    lines << line(tr("arinnd service"), tr("Unavailable"));
    lines << tr("The arinnd service is not responding");
  } else {
    lines << line(tr("arinnd service"), value(m_daemonShort));
    if (!m_daemonBuildId.isEmpty())
      lines << tr("Build %1").arg(m_daemonBuildId);
    if (m_daemonMismatch)
      lines << tr("Different build from Arinn. Restart Arinn");
  }
  lines << line(tr("Channel and build date"), value(m_channelDate));
  if (!m_daemonAvailable)
    lines << line(tr("Providers"), tr("Unavailable"));
  else if (m_providerCount < 0)
    lines << line(tr("Providers"), tr("Unknown"));
  else if (m_providerDetail.isEmpty())
    lines << line(tr("Providers"), QString::number(m_providerCount));
  else
    lines << line(tr("Providers"), m_providerDetail);
  lines << QString();
  lines << tr("System");
  lines << line(tr("OS"), value(m_os));
  lines << line(tr("Kernel"), value(m_kernel));
  lines << line(tr("Plasma"), value(m_plasma));
  lines << line(tr("Qt"), value(m_qt));
  lines << line(tr("Session"), value(m_session));
  lines << QString();
  lines << tr("Display and sound");
  lines << line(tr("Display"), value(m_display));
  lines << line(tr("Audio output"), value(m_audio));
  if (includePrivate) {
    lines << QString();
    lines << tr("Network");
    lines << line(tr("Connection"), value(m_connection));
    lines << line(tr("IP address"), value(m_ip));
  }
  lines << QString();
  lines << tr("Storage");
  lines << line(tr("Free space"), value(m_storage));
  if (includePrivate)
    lines << line(tr("Device name"), value(m_deviceName));
  return lines.join(QLatin1Char('\n')) + QLatin1Char('\n');
}

bool SystemAbout::writeReport(const QString &text) const {
  if (m_fileName.isEmpty() || m_fileName.contains(QLatin1Char('/')) ||
      m_fileName.contains(QLatin1String("..")))
    return false;
  const auto directory = downloadsPath();
  if (directory.isEmpty())
    return false;
  QDir dir(directory);
  if (!dir.mkpath(QStringLiteral(".")))
    return false;
  QSaveFile file(dir.filePath(m_fileName));
  if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate))
    return false;
  const auto bytes = text.toUtf8();
  if (file.write(bytes) != bytes.size())
    return false;
  return file.commit();
}

bool SystemAbout::copyClipboard(const QString &text) const {
  auto *clip = QGuiApplication::clipboard();
  if (!clip)
    return false;
  clip->setText(text, QClipboard::Clipboard);
  return clip->text(QClipboard::Clipboard) == text;
}

int SystemAbout::copyReport(bool includePrivate) {
  if (!m_fixture)
    refreshLocal();
  refreshFileName();
  const auto text = report(includePrivate);
  const bool fileOk = writeReport(text);
  const bool clipOk = copyClipboard(text);
  if (fileOk && clipOk)
    return 0;
  if (!fileOk && clipOk)
    return 1;
  if (fileOk && !clipOk)
    return 2;
  return 3;
}

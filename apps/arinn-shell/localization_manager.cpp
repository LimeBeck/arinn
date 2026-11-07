// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "localization_manager.h"
#include "local_translations.h"
#include <QCoreApplication>
#include <QEvent>
#include <QJsonObject>
#include <QQmlEngine>
#include <QSaveFile>

LocalizationManager::LocalizationManager(QObject *parent)
    : LocalizationManager(LocalTranslations::roots(),
          QDir(QStandardPaths::writableLocation(QStandardPaths::CacheLocation))
              .filePath(QStringLiteral("ui-language.json")), QLocale::system(), parent) {}

LocalizationManager::LocalizationManager(QStringList roots, QString cacheFile,
                                         QLocale systemLocale, QObject *parent)
    : QObject(parent), m_roots(std::move(roots)), m_cacheFile(std::move(cacheFile)),
      m_systemLocale(systemLocale), m_formatLocale(systemLocale) {
  QFile file(m_cacheFile);
  if (!m_cacheFile.isEmpty() && file.open(QIODevice::ReadOnly) && file.size() <= 4096) {
    const auto bytes = file.read(4097);
    const auto object = bytes.size() <= 4096 ? QJsonDocument::fromJson(bytes).object() : QJsonObject{};
    const auto value = object.value(QStringLiteral("language")).toString();
    if (object.value(QStringLiteral("formatVersion")).toInt() == 1 &&
        (value == QLatin1String("system") || LocalTranslations::validLocale(value)))
      m_language = value;
  }
  apply();
}
LocalizationManager::~LocalizationManager() {
  for (const auto &translator : m_translators)
    QCoreApplication::removeTranslator(translator->translator.get());
}
void LocalizationManager::attachEngine(QQmlEngine *engine) { m_engine = engine; }
void LocalizationManager::setLanguage(const QString &language) {
  if (language != QLatin1String("system") && !LocalTranslations::validLocale(language)) return;
  if (language != m_language) {
    m_language = language;
    apply();
  }
  // A derived cache of the daemon-confirmed preference, never a second
  // editable setting and never a source that writes back to the daemon.
  if (!m_cacheFile.isEmpty()) {
    QDir().mkpath(QFileInfo(m_cacheFile).absolutePath());
    QSaveFile file(m_cacheFile);
    if (file.open(QIODevice::WriteOnly)) {
      file.write(QJsonDocument(QJsonObject{{QStringLiteral("formatVersion"), 1},
          {QStringLiteral("language"), m_language}}).toJson(QJsonDocument::Compact));
      file.commit();
    }
  }
}
void LocalizationManager::reload() { apply(); }
QString LocalizationManager::clockText(const QDateTime &time) const {
  return m_formatLocale.toString(time.time(), QLocale::ShortFormat);
}
void LocalizationManager::apply() {
  QStringList languages;
  QLocale formatLocale;
  if (m_language == QLatin1String("system")) {
    languages = m_systemLocale.uiLanguages();
    const auto minimal = m_systemLocale.bcp47Name();
    if (!languages.contains(minimal)) languages.prepend(minimal);
    formatLocale = m_systemLocale;
  } else {
    languages = {m_language};
    const QLocale chosen(m_language);
    formatLocale = !m_language.contains(QLatin1Char('-')) &&
        chosen.language() == m_systemLocale.language() ? m_systemLocale : chosen;
  }
  QString translationLocale = QStringLiteral("en");
  for (auto language : languages) {
    language.replace(QLatin1Char('_'), QLatin1Char('-'));
    if (LocalTranslations::validLocale(language)) {
      translationLocale = language;
      break;
    }
  }
  std::vector<std::unique_ptr<LoadedTranslation>> prepared;
  qint64 externalBytes = 0;
  const auto wasActive = [this](const QString &path) {
    return m_language == m_appliedLanguage && std::any_of(
        m_translators.begin(), m_translators.end(), [&](const auto &entry) { return entry->path == path; });
  };
  // Removal is intentional and enables fallback. An invalid replacement of
  // an active catalogue keeps the last good in-memory version instead.
  for (const auto &entry : m_translators) {
    if (!entry->path.startsWith(QLatin1String(":/")) && wasActive(entry->path) &&
        QFileInfo::exists(entry->path) &&
        (QFileInfo(entry->path).size() <= 0 || QFileInfo(entry->path).size() > LocalTranslations::maxCatalogBytes)) {
      m_reloadFailed = true;
      emit changed();
      return;
    }
  }
  for (const auto &domain : {QStringLiteral("arinn"), QStringLiteral("music")}) {
    QStringList paths;
    for (auto language : languages) {
      language.replace(QLatin1Char('_'), QLatin1Char('-'));
      if (!LocalTranslations::validLocale(language)) continue;
      paths.append(LocalTranslations::catalogPaths(m_roots, domain, language));
      for (auto locale : LocalTranslations::locales(language)) {
        locale.replace(QLatin1Char('-'), QLatin1Char('_'));
        const auto resource = QStringLiteral(":/i18n/%1_%2.qm").arg(domain, locale);
        if (QFileInfo::exists(resource)) paths.append(resource);
      }
    }
    paths.removeDuplicates();
    for (const auto &path : paths) {
      auto entry = std::make_unique<LoadedTranslation>();
      entry->path = path;
      QFile file(path);
      if (file.open(QIODevice::ReadOnly))
        entry->bytes = file.read(LocalTranslations::maxCatalogBytes + 1);
      const bool resource = path.startsWith(QLatin1String(":/"));
      entry->translator = std::make_unique<QTranslator>();
      const bool valid = !entry->bytes.isEmpty() &&
          entry->bytes.size() <= LocalTranslations::maxCatalogBytes &&
          (resource || externalBytes + entry->bytes.size() <= 8 * LocalTranslations::maxCatalogBytes) &&
          LocalTranslations::selfContainedQm(entry->bytes) &&
          entry->translator->load(reinterpret_cast<const uchar *>(entry->bytes.constData()), entry->bytes.size());
      if (valid) {
        if (!resource) externalBytes += entry->bytes.size();
        prepared.push_back(std::move(entry));
      } else if (wasActive(path)) {
        m_reloadFailed = true;
        emit changed();
        return;
      }
    }
  }
  for (const auto &translator : m_translators)
    QCoreApplication::removeTranslator(translator->translator.get());
  m_translators = std::move(prepared);
  m_formatLocale = formatLocale;
  m_translationLocale = translationLocale;
  QLocale::setDefault(m_formatLocale);
  for (auto it = m_translators.rbegin(); it != m_translators.rend(); ++it)
    QCoreApplication::installTranslator((*it)->translator.get());
  m_appliedLanguage = m_language;
  m_reloadFailed = false;
  ++m_revision;
  QEvent event(QEvent::LanguageChange);
  QCoreApplication::sendEvent(QCoreApplication::instance(), &event);
  if (m_engine) m_engine->retranslate();
  emit changed();
}
